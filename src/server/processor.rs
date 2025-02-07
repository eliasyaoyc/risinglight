use std::io;

use async_trait::async_trait;
use futures::stream;
use pgwire::api::query::SimpleQueryHandler;
use pgwire::api::results::{query_response, DataRowEncoder, FieldFormat, FieldInfo, Response, Tag};
use pgwire::api::{ClientInfo, Type};
use pgwire::error::{PgWireError, PgWireResult};
use tokio::{select, signal};
use tracing::log::info;

use crate::Database;

pub struct Processor {
    db: Database,
}

impl Processor {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl SimpleQueryHandler for Processor {
    async fn do_query<C>(&self, _client: &C, query: &str) -> PgWireResult<Vec<Response>>
    where
        C: ClientInfo + Unpin + Send + Sync,
    {
        info!("query:{query:?}");
        let task = async move { self.db.run(query).await };

        select! {
            _ = signal::ctrl_c() => {
                // we simply drop the future `task` to cancel the query.
                info!("Interrupted");
                return Err(io::Error::new(io::ErrorKind::Interrupted, "Interrupted").into());
            }
            ret = task => {
                ret.map(|chunks|{
                    if !query.to_uppercase().starts_with("SELECT"){
                        return vec![Response::Execution(Tag::new_for_execution("OK", None))];
                    }
                    let mut results = Vec::new();
                    let mut col_num = 0;
                    for chunk in chunks {
                        for data_chunk in chunk.data_chunks() {
                            for i in 0..data_chunk.cardinality() {
                                col_num = data_chunk.arrays().len();
                                let mut encoder = DataRowEncoder::new(col_num);
                                data_chunk.arrays().iter().for_each(|a| {
                                    let field = a.get_to_string(i);
                                    encoder.encode_text_format_field(Some(&field)).unwrap();
                                });
                                results.push(encoder.finish());
                            }
                        }
                    }
                    let headers = vec![
                        FieldInfo::new("++".into(), None, None, Type::CHAR, FieldFormat::Text);col_num
                    ];
                    vec![Response::Query(query_response(
                        Some(headers),
                        stream::iter(results.into_iter()),
                    ))]
                }).map_err(|e| PgWireError::ApiError(Box::new(e)))
            }
        }
    }
}
