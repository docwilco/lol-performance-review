use crate::{ApiRegion, Result};
use dashmap::{DashMap, Entry};
use governor::{DefaultDirectRateLimiter, Quota};
use log::{debug, trace};
use nonzero_ext::nonzero;
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Client, Url,
};
use serde::Deserialize;
use std::{fmt, fs, num::NonZeroU32, sync::Arc, time::Duration};
use tokio::time::sleep;

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
}

impl ApiClient {
    pub fn new(api_key: &str) -> Result<Self> {
        // We always put in the same API token
        let mut headers = HeaderMap::new();
        let mut api_key_value = HeaderValue::from_str(api_key)?;
        api_key_value.set_sensitive(true);
        headers.insert("X-Riot-Token", api_key_value);
        let client = Client::builder().default_headers(headers).build()?;
        let app_per_second = Arc::new(DefaultDirectRateLimiter::direct(Quota::per_minute(
            nonzero!(49_u32),
        )));
        let app_per_2_minutes = Arc::new(DefaultDirectRateLimiter::direct(Quota::per_second(
            nonzero!(19_u32),
        )));
        let app_limits = [app_per_second, app_per_2_minutes];
        let method_limits = Arc::new(DashMap::new());
        Ok(Self {
            http_client: client,
            app_limits,
            method_limits,
        })
    }

    pub async fn get<T>(
        &self,
        region: ApiRegion,
        method: &str,
        path_params: impl IntoIterator<Item = &str> + fmt::Debug,
    ) -> Result<T>
    where
        T: for<'a> Deserialize<'a>,
    {
        self.get_with_query(region, method, path_params, vec![])
            .await
    }

    pub async fn get_with_query<T>(
        &self,
        region: ApiRegion,
        method: &str,
        path_params: impl IntoIterator<Item = &str> + fmt::Debug,
        query_params: impl IntoIterator<Item = (&str, &str)> + fmt::Debug,
    ) -> Result<T>
    where
        T: for<'a> Deserialize<'a>,
    {
        debug!("GET {} {:?} {:?}", method, path_params, query_params);
        let mut url = make_url(region, method, path_params)?;
        let mut query_params = query_params.into_iter().peekable();
        if query_params.peek().is_some() {
            url.query_pairs_mut().extend_pairs(query_params);
        }

        // Rate limiting
        for limit in &self.app_limits {
            limit.until_ready().await;
        }
        let method_limit = self.method_limits.entry(method.to_string());
        if let Entry::Occupied(ref entry) = method_limit {
            entry.get().until_ready().await;
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
        // Make a new rate limiter if we didn't have one for this method
        if let Entry::Vacant(entry) = method_limit {
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
            debug!(
                "Setting rate limit for {} to {}/hour",
                method, rate_per_hour
            );
            entry.insert(limit);
        }

        while let Some(retry_after) = response.headers().get("retry-after") {
            let retry_after = retry_after.to_str()?.parse::<u64>()?;
            debug!("Rate limited, retrying in {} seconds", retry_after);
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
