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

#![allow(clippy::uninlined_format_args)]
#![deny(unused_crate_dependencies)]

mod limiter_pruner;
mod page_pruner;
mod range_pruner;
mod topn_pruner;

pub use limiter_pruner::LimiterPruner;
pub use limiter_pruner::LimiterPrunerCreator;
pub use page_pruner::PagePruner;
pub use page_pruner::PagePrunerCreator;
pub use range_pruner::RangePruner;
pub use range_pruner::RangePrunerCreator;
pub use topn_pruner::BlockMetaIndex;
pub use topn_pruner::TopNPrunner;
