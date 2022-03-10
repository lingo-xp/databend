//  Copyright 2022 Datafuse Labs.
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

use std::io::ErrorKind;

use common_exception::ErrorCode;
use common_exception::Result;
use futures::AsyncRead;
use serde::de::DeserializeOwned;
use serde_json::from_slice;

use crate::storages::fuse::meta::SegmentInfo;
use crate::storages::fuse::meta::SegmentInfoVersions;
use crate::storages::fuse::meta::SnapshotVersions;
use crate::storages::fuse::meta::TableSnapshot;
use crate::storages::fuse::meta::Versioned;

#[async_trait::async_trait]
pub trait VersionedLoader<T> {
    async fn vload<R>(&self, read: R, location: &str, len_hint: Option<u64>) -> Result<T>
    where R: AsyncRead + Unpin + Send;
}

#[async_trait::async_trait]
impl VersionedLoader<TableSnapshot> for SnapshotVersions {
    async fn vload<R>(
        &self,
        reader: R,
        _location: &str,
        _len_hint: Option<u64>,
    ) -> Result<TableSnapshot>
    where
        R: AsyncRead + Unpin + Send,
    {
        let r = match self {
            SnapshotVersions::V1(v) => do_load(reader, v).await?,
            SnapshotVersions::V0(v) => do_load(reader, v).await?.into(),
        };
        Ok(r)
    }
}

#[async_trait::async_trait]
impl VersionedLoader<SegmentInfo> for SegmentInfoVersions {
    async fn vload<R>(
        &self,
        reader: R,
        _location: &str,
        _len_hint: Option<u64>,
    ) -> Result<SegmentInfo>
    where
        R: AsyncRead + Unpin + Send,
    {
        let r = match self {
            SegmentInfoVersions::V1(v) => do_load(reader, v).await?,
            SegmentInfoVersions::V0(v) => do_load(reader, v).await?.into(),
        };
        Ok(r)
    }
}

async fn do_load<R, T>(mut reader: T, _v: &Versioned<R>) -> Result<R>
where
    R: DeserializeOwned,
    T: AsyncRead + Unpin + Send,
{
    let mut buffer: Vec<u8> = vec![];
    use futures::AsyncReadExt;
    reader.read_to_end(&mut buffer).await.map_err(|e| {
        let msg = e.to_string();
        if e.kind() == ErrorKind::NotFound {
            ErrorCode::DalPathNotFound(msg)
        } else {
            ErrorCode::DalTransportError(msg)
        }
    })?;
    Ok(from_slice::<R>(&buffer)?)
}
