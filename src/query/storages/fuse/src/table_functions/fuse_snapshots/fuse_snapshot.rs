//  Copyright 2021 Datafuse Labs.
//
//  Licensed under the Apache License, Version 2.0 (the "License");
//  you may not use this file except in compliance with the License.
//  You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.

use std::sync::Arc;

use common_datablocks::DataBlock;
use common_datavalues::prelude::*;
use common_exception::Result;
use common_fuse_meta::meta::TableSnapshotLite;

use crate::fuse_snapshot::read_snapshots_by_root_file;
use crate::io::TableMetaLocationGenerator;
use crate::sessions::TableContext;
use crate::FuseTable;

pub struct FuseSnapshot<'a> {
    pub ctx: Arc<dyn TableContext>,
    pub table: &'a FuseTable,
}

impl<'a> FuseSnapshot<'a> {
    pub fn new(ctx: Arc<dyn TableContext>, table: &'a FuseTable) -> Self {
        Self { ctx, table }
    }

    pub async fn get_snapshots(self) -> Result<DataBlock> {
        let meta_location_generator = self.table.meta_location_generator.clone();
        let snapshot_location = self.table.snapshot_loc();
        if let Some(snapshot_location) = snapshot_location {
            let snapshot_version = self.table.snapshot_format_version();
            let (snapshots, _) = read_snapshots_by_root_file(
                self.ctx.clone(),
                snapshot_location,
                snapshot_version,
                &self.table.operator,
                false,
            )
            .await?;
            return self.to_block(&meta_location_generator, &snapshots, snapshot_version);
        }
        Ok(DataBlock::empty_with_schema(FuseSnapshot::schema()))
    }

    fn to_block(
        &self,
        location_generator: &TableMetaLocationGenerator,
        snapshots: &[TableSnapshotLite],
        latest_snapshot_version: u64,
    ) -> Result<DataBlock> {
        let len = snapshots.len();
        let mut snapshot_ids: Vec<Vec<u8>> = Vec::with_capacity(len);
        let mut snapshot_locations: Vec<Vec<u8>> = Vec::with_capacity(len);
        let mut prev_snapshot_ids: Vec<Option<Vec<u8>>> = Vec::with_capacity(len);
        let mut format_versions: Vec<u64> = Vec::with_capacity(len);
        let mut segment_count: Vec<u32> = Vec::with_capacity(len);
        let mut block_count: Vec<u32> = Vec::with_capacity(len);
        let mut row_count: Vec<u32> = Vec::with_capacity(len);
        let mut compressed: Vec<u32> = Vec::with_capacity(len);
        let mut uncompressed: Vec<u32> = Vec::with_capacity(len);
        let mut index_size: Vec<u32> = Vec::with_capacity(len);
        let mut timestamps: Vec<Option<i64>> = Vec::with_capacity(len);
        let mut current_snapshot_version = latest_snapshot_version;
        for s in snapshots {
            snapshot_ids.push(s.snapshot_id.simple().to_string().into_bytes());
            snapshot_locations.push(
                location_generator
                    .snapshot_location_from_uuid(&s.snapshot_id, current_snapshot_version)?
                    .into_bytes(),
            );
            let (id, ver) = match s.prev_snapshot_id {
                Some((id, v)) => (Some(id.simple().to_string().into_bytes()), v),
                None => (None, 0),
            };
            prev_snapshot_ids.push(id);
            format_versions.push(s.format_version);
            segment_count.push(s.segment_count);
            block_count.push(s.block_count);
            row_count.push(s.row_count);
            compressed.push(s.compressed_byte_size);
            uncompressed.push(s.uncompressed_byte_size);
            index_size.push(s.index_size);
            timestamps.push(s.timestamp.map(|dt| (dt.timestamp_micros()) as i64));
            current_snapshot_version = ver;
        }

        Ok(DataBlock::create(FuseSnapshot::schema(), vec![
            Series::from_data(snapshot_ids),
            Series::from_data(snapshot_locations),
            Series::from_data(format_versions),
            Series::from_data(prev_snapshot_ids),
            Series::from_data(segment_count),
            Series::from_data(block_count),
            Series::from_data(row_count),
            Series::from_data(uncompressed),
            Series::from_data(compressed),
            Series::from_data(index_size),
            Series::from_data(timestamps),
        ]))
    }

    pub fn schema() -> Arc<DataSchema> {
        DataSchemaRefExt::create(vec![
            DataField::new("snapshot_id", Vu8::to_data_type()),
            DataField::new("snapshot_location", Vu8::to_data_type()),
            DataField::new("format_version", u64::to_data_type()),
            DataField::new_nullable("previous_snapshot_id", Vu8::to_data_type()),
            DataField::new("segment_count", u32::to_data_type()),
            DataField::new("block_count", u32::to_data_type()),
            DataField::new("row_count", u32::to_data_type()),
            DataField::new("bytes_uncompressed", u32::to_data_type()),
            DataField::new("bytes_compressed", u32::to_data_type()),
            DataField::new("index_size", u32::to_data_type()),
            DataField::new_nullable("timestamp", TimestampType::new_impl(6)),
        ])
    }
}
