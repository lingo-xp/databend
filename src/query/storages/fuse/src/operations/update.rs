// Copyright 2023 Datafuse Labs.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::collections::BTreeMap;
use std::sync::Arc;

use common_catalog::plan::Projection;
use common_catalog::table::Table;
use common_catalog::table_context::TableContext;
use common_exception::Result;
use common_expression::RemoteExpr;
use common_expression::TableDataType;
use common_expression::TableField;
use common_expression::TableSchema;
use common_functions::scalars::BUILTIN_FUNCTIONS;
use common_sql::evaluator::BlockOperator;
use storages_common_table_meta::meta::TableSnapshot;

use crate::operations::mutation::MutationAction;
use crate::operations::mutation::MutationSink;
use crate::operations::mutation::MutationSource;
use crate::operations::mutation::SerializeDataTransform;
use crate::pipelines::Pipeline;
use crate::statistics::ClusterStatsGenerator;
use crate::FuseTable;

impl FuseTable {
    /// UPDATE column = expression WHERE condition
    /// The flow of Pipeline is the same as that of deletion.
    pub async fn do_update(
        &self,
        ctx: Arc<dyn TableContext>,
        filter: Option<RemoteExpr<String>>,
        col_indices: Vec<usize>,
        update_list: Vec<(usize, RemoteExpr<String>)>,
        pipeline: &mut Pipeline,
    ) -> Result<()> {
        let snapshot_opt = self.read_table_snapshot().await?;

        // check if table is empty
        let snapshot = if let Some(val) = snapshot_opt {
            val
        } else {
            // no snapshot, no update
            return Ok(());
        };

        if snapshot.summary.row_count == 0 {
            // empty snapshot, no update
            return Ok(());
        }

        let mut filter = filter;
        if col_indices.is_empty() && filter.is_some() {
            let filter_expr = filter.clone().unwrap();
            if !self.try_eval_const(ctx.clone(), &self.schema(), &filter_expr)? {
                // The condition is always false, do nothing.
                return Ok(());
            }
            // The condition is always true.
            filter = None;
        }

        self.try_add_update_source(
            ctx.clone(),
            filter,
            col_indices,
            update_list,
            &snapshot,
            pipeline,
        )
        .await?;

        // TODO(zhyass): support cluster stats generator.
        pipeline.add_transform(|input, output| {
            SerializeDataTransform::try_create(
                ctx.clone(),
                input,
                output,
                self,
                ClusterStatsGenerator::default(),
            )
        })?;

        self.try_add_mutation_transform(ctx.clone(), snapshot.segments.clone(), pipeline)?;

        pipeline.add_sink(|input| {
            MutationSink::try_create(self, ctx.clone(), snapshot.clone(), input)
        })?;
        Ok(())
    }

    async fn try_add_update_source(
        &self,
        ctx: Arc<dyn TableContext>,
        filter: Option<RemoteExpr<String>>,
        col_indices: Vec<usize>,
        update_list: Vec<(usize, RemoteExpr<String>)>,
        base_snapshot: &TableSnapshot,
        pipeline: &mut Pipeline,
    ) -> Result<()> {
        let all_col_ids = self.all_the_columns_ids();
        let schema = self.schema();

        let mut offset_map = BTreeMap::new();
        let mut remain_reader = None;
        let mut pos = 0;
        let (projection, input_schema) = if col_indices.is_empty() {
            all_col_ids.iter().for_each(|&id| {
                offset_map.insert(id, pos);
                pos += 1;
            });

            (Projection::Columns(all_col_ids), schema.clone())
        } else {
            col_indices.iter().for_each(|&id| {
                offset_map.insert(id, pos);
                pos += 1;
            });

            let mut fields: Vec<TableField> = col_indices
                .iter()
                .map(|idx| schema.fields()[*idx].clone())
                .collect();

            let remain_col_ids: Vec<usize> = all_col_ids
                .into_iter()
                .filter(|id| !col_indices.contains(id))
                .collect();
            if !remain_col_ids.is_empty() {
                remain_col_ids.iter().for_each(|&id| {
                    offset_map.insert(id, pos);
                    pos += 1;
                });

                let reader = self.create_block_reader(Projection::Columns(remain_col_ids))?;
                fields.extend_from_slice(reader.schema().fields());
                remain_reader = Some((*reader).clone());
            }

            fields.push(TableField::new("_predicate", TableDataType::Boolean));
            pos += 1;

            (
                Projection::Columns(col_indices.clone()),
                Arc::new(TableSchema::new(fields)),
            )
        };

        let mut ops = Vec::with_capacity(update_list.len() + 1);
        for (id, remote_expr) in update_list.into_iter() {
            let expr = remote_expr
                .as_expr(&BUILTIN_FUNCTIONS)
                .project_column_ref(|name| input_schema.index_of(name).unwrap());
            ops.push(BlockOperator::Map { expr });
            offset_map.insert(id, pos);
            pos += 1;
        }
        ops.push(BlockOperator::Project {
            projection: offset_map.values().cloned().collect(),
        });

        let block_reader = self.create_block_reader(projection.clone())?;
        let remain_reader = Arc::new(remain_reader);
        let (filter_expr, filters) = if let Some(remote_expr) = filter {
            let schema = block_reader.schema();
            (
                Arc::new(Some(
                    remote_expr
                        .as_expr(&BUILTIN_FUNCTIONS)
                        .project_column_ref(|name| schema.index_of(name).unwrap()),
                )),
                vec![remote_expr],
            )
        } else {
            (Arc::new(None), vec![])
        };

        self.mutation_block_purning(ctx.clone(), filters, projection, base_snapshot)
            .await?;

        let max_threads = ctx.get_settings().get_max_threads()? as usize;
        // Add source pipe.
        pipeline.add_source(
            |output| {
                MutationSource::try_create(
                    ctx.clone(),
                    MutationAction::Update,
                    output,
                    filter_expr.clone(),
                    block_reader.clone(),
                    remain_reader.clone(),
                    ops.clone(),
                )
            },
            max_threads,
        )
    }
}
