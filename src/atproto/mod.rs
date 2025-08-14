#[cfg(feature = "blog")]
pub mod com;

#[cfg(feature = "blog")]
pub mod blog {
    use std::str::FromStr as _;

    use atrium_api::agent::atp_agent::store::MemorySessionStore;
    use atrium_api::agent::atp_agent::CredentialSession;
    use atrium_api::agent::Agent;
    use atrium_api::com::atproto::repo::list_records;
    use atrium_api::com::atproto::server::create_session::OutputData as SessionOutputData;
    use atrium_api::types::string::{AtIdentifier, Handle};
    use atrium_api::types::{Collection as _, Object, Unknown};
    use atrium_common::store::memory::MemoryStore;
    use atrium_common::store::Store;
    use atrium_xrpc_client::reqwest::ReqwestClient;
    use color_eyre::eyre::eyre;
    use color_eyre::Result;
    use ipld_core::ipld::Ipld;
    use lazy_static::lazy_static;
    use tokio::time::{Duration, Instant};
    use tracing::instrument;

    use super::*;

    const CACHE_INVALIDATION_PERIOD: Duration = Duration::from_secs(30 * 60); // 30 minutes
    lazy_static! {
        static ref POSTS_CACHE_STORE: MemoryStore<usize, (Instant, com::whtwnd::blog::entry::Record)> =
            MemoryStore::default();
        static ref AGENT: Agent<
            CredentialSession<
                MemoryStore<(), Object<SessionOutputData>>,
                ReqwestClient,
            >,
        > = Agent::new(CredentialSession::new(
            ReqwestClient::new("https://bsky.social"),
            MemorySessionStore::default(),
        ));
    }

    #[instrument(level = "debug")]
    pub async fn get_all_posts() -> Result<Vec<com::whtwnd::blog::entry::Record>>
    {
        let mut i = 0;
        let mut posts = Vec::new();
        while let Some((cache_creation_time, post)) =
            POSTS_CACHE_STORE.get(&i).await?
        {
            if cache_creation_time.elapsed() > CACHE_INVALIDATION_PERIOD {
                tracing::info!(
                    "Cache for post #{} is stale, fetching new posts",
                    i
                );
                POSTS_CACHE_STORE.clear().await?;
                return fetch_posts_into_cache().await;
            }

            posts.push(post);

            i += 1;
        }

        if posts.is_empty() {
            tracing::info!(
                "No blog posts found in cache, fetching from ATProto"
            );
            return fetch_posts_into_cache().await;
        }

        Ok(posts)
    }

    #[instrument(level = "trace")]
    async fn fetch_posts_into_cache(
    ) -> Result<Vec<com::whtwnd::blog::entry::Record>> {
        let records = &AGENT
            .api
            .com
            .atproto
            .repo
            .list_records(list_records::Parameters {
                extra_data: Ipld::Null,
                data: list_records::ParametersData {
                    collection: com::whtwnd::blog::Entry::nsid(),
                    cursor: None,
                    limit: None,
                    reverse: None,
                    repo: AtIdentifier::Handle(
                        Handle::from_str("devcomp.xyz")
                            .map_err(|_| eyre!("Invalid repo handle"))?,
                    ),
                },
            })
            .await?
            .records;

        let posts = records
            .iter()
            .map(|elem| {
                if let Unknown::Object(btree_map) = &elem.data.value {
                    let ser = serde_json::to_string(&btree_map)?;
                    let des = serde_json::from_str::<
                        com::whtwnd::blog::entry::Record,
                    >(&ser)?;

                    return Ok(des);
                }

                Err(eyre!("Did not get posts back from atproto"))
            })
            .collect::<Result<Vec<com::whtwnd::blog::entry::Record>>>()?;

        for (i, post) in posts.iter().enumerate() {
            POSTS_CACHE_STORE.set(i, (Instant::now(), post.clone())).await?;
        }

        Ok(posts)
    }
}
