use crate::{ApiRegion, FetchStatusPerPlayer, Player, Result};
use dashmap::DashMap;
use governor::{DefaultDirectRateLimiter, Quota};
use log::{debug, trace};
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Client, Url,
};
use serde::Deserialize;
use std::{env, fmt, fs, num::NonZeroU32, sync::Arc, time::Duration};
use tokio::time::{interval, sleep};

//#[derive(Deserialize)]
//struct Status {
//    message: String,
//    status_code: u16,
//}

#[derive(Clone, Debug)]
pub struct ApiClient {
    // Reqwest client already uses an Arc internally, so we don't need to wrap it in an Arc
    http_client: reqwest::Client,
    app_limits: [Arc<DefaultDirectRateLimiter>; 2],
    method_limits: Arc<DashMap<String, Arc<DefaultDirectRateLimiter>>>,
    fetch_status_per_player: FetchStatusPerPlayer,
}

impl ApiClient {
    pub fn new(api_key: &str, fetch_status_per_player: FetchStatusPerPlayer) -> Result<Self> {
        // We always put in the same API token
        let mut headers = HeaderMap::new();
        let mut api_key_value = HeaderValue::from_str(api_key)?;
        api_key_value.set_sensitive(true);
        headers.insert("X-Riot-Token", api_key_value);
        let client = Client::builder().default_headers(headers).build()?;
        let first_app_limit = env::var("RATELIMIT_APP_LIMIT_1")?.parse::<u32>()?;
        let second_app_limit = env::var("RATELIMIT_APP_LIMIT_2")?.parse::<u32>()?;
        let first_app_duration = duration_str::parse(env::var("RATELIMIT_APP_DURATION_1")?)?;
        let second_app_duration = duration_str::parse(env::var("RATELIMIT_APP_DURATION_2")?)?;
        let first_period_per: Duration = first_app_duration / first_app_limit;
        let second_period_per: Duration = second_app_duration / second_app_limit;
        let first_app_rate = Arc::new(DefaultDirectRateLimiter::direct(
            Quota::with_period(first_period_per).unwrap(),
        ));
        let second_app_rate = Arc::new(DefaultDirectRateLimiter::direct(
            Quota::with_period(second_period_per).unwrap(),
        ));
        let app_limits = [first_app_rate, second_app_rate];
        let method_limits = Arc::new(DashMap::new());
        Ok(Self {
            http_client: client,
            app_limits,
            method_limits,
            fetch_status_per_player,
        })
    }

    pub async fn get<T>(
        &self,
        region: ApiRegion,
        method: &str,
        path_params: impl IntoIterator<Item = &str> + fmt::Debug,
        player: &Player,
    ) -> Result<T>
    where
        T: for<'a> Deserialize<'a>,
    {
        self.get_with_query(region, method, path_params, vec![], player)
            .await
    }

    pub async fn get_with_query<T>(
        &self,
        region: ApiRegion,
        method: &str,
        path_params: impl IntoIterator<Item = &str> + fmt::Debug,
        query_params: impl IntoIterator<Item = (&str, &str)> + fmt::Debug,
        player: &Player,
    ) -> Result<T>
    where
        T: for<'a> Deserialize<'a>,
    {
        debug!("GET {method} {path_params:?} {query_params:?}");
        let mut url = make_url(region, method, path_params)?;
        let mut query_params = query_params.into_iter().peekable();
        if query_params.peek().is_some() {
            url.query_pairs_mut().extend_pairs(query_params);
        }

        // Rate limiting
        for limit in &self.app_limits {
            limit.until_ready().await;
        }
        let method_limit = self.method_limits.get(method);
        let method_limit = method_limit.map(|limit| limit.clone());
        if let Some(limit) = method_limit {
            limit.until_ready().await;
        }
        // Execute the request
        let mut response = self.http_client.get(url.clone()).send().await?;

        // Debug rate limiter headers
        response
            .headers()
            .iter()
            .filter(|(name, _)| name.as_str().contains("rate"))
            .for_each(|(name, value)| {
                trace!("{}: {}", name, value.to_str().unwrap());
            });
        // Make a new rate limiter if we don't have one for this method.
        // Re-get from the map, because someone else might have inserted it
        // while we were waiting on the server
        let method_limit = self.method_limits.get(method);
        if method_limit.is_none() {
            let limit = response
                .headers()
                .get("X-Method-Rate-Limit")
                .map(|value| value.to_str())
                .transpose()?
                .unwrap_or("20:1");
            let mut parts = limit.split(':');
            let rate = parts.next().unwrap().parse::<u32>()?;
            let per = parts.next().unwrap().parse::<u32>()?;
            let factor = 3600 / per;
            let rate_per_hour = rate * factor;
            let limit = Arc::new(DefaultDirectRateLimiter::direct(Quota::per_hour(
                NonZeroU32::new(rate_per_hour).ok_or("rate_per_hour is zero")?,
            )));
            // Shouldn't fail, because the limiter is as fresh as can be. We
            // check because we want to set the limiter to have a count of 1 for
            // the request we just did.
            assert!(limit.check().is_ok());
            debug!("Setting rate limit for {method} to {rate_per_hour}/hour");
            self.method_limits.insert(method.to_string(), limit);
        }

        while let Some(retry_after) = response.headers().get("retry-after") {
            let mut retry_after = retry_after.to_str()?.parse::<u64>()?;
            debug!("Rate limited, retrying in {retry_after} seconds");
            let mut interval = interval(Duration::from_secs(1));
            while retry_after > 0 {
                let broadcaster = self.fetch_status_per_player.get_mut(player);
                if let Some(mut broadcaster) = broadcaster {
                    broadcaster
                        .broadcast(crate::fetcher::FetchStatus::Waiting {
                            seconds_left: retry_after,
                        })
                        .await;
                }
                interval.tick().await;
                retry_after -= 1;
            }
            sleep(Duration::from_secs(retry_after)).await;
            response = self.http_client.get(url.clone()).send().await?;
        }

        let headers = response.headers().clone();
        let text = response.text().await?;

        let result = serde_json::from_str(&text);
        if let Err(e) = result {
            println!("Headers: {headers:?}");
            // write to debug.json
            fs::write("debug.json", text)?;
            return Err(e.into());
        }
        Ok(result?)
        //Ok(self.http_client.get(url).send().await?.json::<T>().await.unwrap())
    }
}

fn make_url<'a>(
    region: ApiRegion,
    method: &str,
    path_params: impl IntoIterator<Item = &'a str>,
) -> Result<Url> {
    let mut url = format!("https://{hostname}{method}", hostname = region.hostname());
    for arg in path_params {
        url.push('/');
        url.push_str(arg);
    }
    Url::parse(&url).map_err(Into::into)
}
