use crate::*;
use futures::lock::Mutex;
use lazy_static::lazy_static;
use sandbox::AsyncSandbox;
use std::{
    collections::hash_map::DefaultHasher,
    future::Future,
    hash::{Hash, Hasher},
    net::IpAddr,
};
use warp::{
    body, fs, header,
    http::{self, StatusCode},
    reject,
    reply::{self, Json, Reply},
    Filter, Rejection,
};

// TODO: format errors correctly

#[tokio::main]
pub(crate) async fn server_warp(config: ServerConfig) {
    let ServerConfig {
        root,
        gh_token,
        address,
        port,
        logfile,
        cors_enabled,
    } = config;
    let address: IpAddr = address.parse().expect("Unable to parse server address");

    // TODO: add caching; etags; side-by-side compression?
    let all_files = warp::get().and(fs::dir(root));

    let execute = warp::path("execute").and(sandbox_api(execute));
    let compile = warp::path("compile").and(sandbox_api(compile));
    let format = warp::path("format").and(sandbox_api(format));
    let clippy = warp::path("clippy").and(sandbox_api(clippy));
    let miri = warp::path("miri").and(sandbox_api(miri));

    let api = execute.or(compile).or(format).or(clippy).or(miri);
    let api = warp::post().and(api);

    let crates = warp::path("crates").and(cached_api(AsyncSandboxCache::crates));

    let stable = warp::path("stable").and(cached_api(AsyncSandboxCache::version_stable));
    let beta = warp::path("beta").and(cached_api(AsyncSandboxCache::version_beta));
    let nightly = warp::path("nightly").and(cached_api(AsyncSandboxCache::version_nightly));
    let rustfmt = warp::path("rustfmt").and(cached_api(AsyncSandboxCache::version_rustfmt));
    let clippy = warp::path("clippy").and(cached_api(AsyncSandboxCache::version_clippy));
    let miri = warp::path("miri").and(cached_api(AsyncSandboxCache::version_miri));

    let version = stable.or(beta).or(nightly).or(rustfmt).or(clippy).or(miri);
    let version = warp::path("version").and(version);

    //mount.mount("/meta/gist", gist_router);
    //mount.mount("/evaluate.json", evaluate);

    let meta = crates.or(version);
    let meta = warp::path("meta").and(meta);
    let meta = warp::get().and(meta);

    let route = api.or(meta).or(all_files);

    let server = warp::serve(route).run((address, port));

    server.await;
}

// TODO: JSON content type only?; content_length_limit?
fn sandbox_api<F, Req, FutResp, Resp>(
    f: F,
) -> impl Filter<Extract = (Json,), Error = Rejection> + Clone
where
    F: Fn(Req) -> FutResp,
    F: Copy + Clone + Send + Sync,
    Req: DeserializeOwned + Send,
    FutResp: Future<Output = Result<Resp>> + Send + Sync,
    Resp: Serialize,
{
    body::json().and_then(move |req| async move {
        f(req)
            .await
            .map(|r| reply::json(&r))
            .map_err(reject::custom)
    })
}

lazy_static! {
    static ref CACHED_DATA: AsyncSandboxCache = Default::default();
}

fn cached_api<F, CacheResp, Resp>(
    f: F,
) -> impl Filter<Extract = (Box<dyn Reply>,), Error = Rejection> + Clone
where
    F: Fn(&'static AsyncSandboxCache, Option<Etag>) -> CacheResp,
    F: Copy + Clone + Send + Sync + 'static,
    CacheResp: Future<Output = Result<Changed<Resp>>> + Send + Sync,
    Resp: Serialize,
{
    header::optional(http::header::IF_NONE_MATCH.as_str()).and_then(move |client_etag| async move {
        f(&*CACHED_DATA, client_etag)
            .await
            .map(reply_changed)
            .map_err(reject::custom)
    })
}

fn reply_changed<T>(r: Changed<T>) -> Box<dyn reply::Reply>
where
    T: Serialize,
{
    match r {
        Changed::Changed(r, e) => {
            let j = reply::json(&r);
            let h = reply::with_header(j, http::header::ETAG, e);
            Box::new(h)
        }
        Changed::Unchanged => {
            let e = reply::reply();
            let s = reply::with_status(e, StatusCode::NOT_MODIFIED);
            Box::new(s)
        }
    }
}

/* ---------- */

async fn with_sandbox<F, FutRes, Res>(f: F) -> Result<Res>
where
    F: FnOnce(AsyncSandbox) -> FutRes,
    FutRes: Future<Output = Result<Res>>,
{
    let sandbox = AsyncSandbox::new().await.context(SandboxCreation)?;
    f(sandbox).await
}

async fn execute(req: ExecuteRequest) -> Result<ExecuteResponse> {
    with_sandbox(|sandbox| async {
        let resp = sandbox.execute(req.try_into()?).await.context(Execution)?;
        Ok(resp.into())
    })
    .await
}

async fn compile(req: CompileRequest) -> Result<CompileResponse> {
    with_sandbox(|sandbox| async {
        let resp = sandbox
            .compile(req.try_into()?)
            .await
            .context(Compilation)?;
        Ok(resp.into())
    })
    .await
}

async fn format(req: FormatRequest) -> Result<FormatResponse> {
    with_sandbox(|sandbox| async {
        let resp = sandbox.format(req.try_into()?).await.context(Formatting)?;
        Ok(resp.into())
    })
    .await
}

async fn clippy(req: ClippyRequest) -> Result<ClippyResponse> {
    with_sandbox(|sandbox| async {
        let resp = sandbox.clippy(req.try_into()?).await.context(Linting)?;
        Ok(resp.into())
    })
    .await
}

async fn miri(req: MiriRequest) -> Result<MiriResponse> {
    with_sandbox(|sandbox| async {
        let resp = sandbox.miri(req.try_into()?).await.context(Interpreting)?;
        Ok(resp.into())
    })
    .await
}

type Etag = String;

#[derive(Debug, Default)]
struct AsyncSandboxCache {
    crates: AsyncSandboxCacheOne<Vec<sandbox::CrateInformation>>,
    version_stable: AsyncSandboxCacheOne<sandbox::Version>,
    version_beta: AsyncSandboxCacheOne<sandbox::Version>,
    version_nightly: AsyncSandboxCacheOne<sandbox::Version>,
    version_rustfmt: AsyncSandboxCacheOne<sandbox::Version>,
    version_clippy: AsyncSandboxCacheOne<sandbox::Version>,
    version_miri: AsyncSandboxCacheOne<sandbox::Version>,
}

impl AsyncSandboxCache {
    async fn crates(&self, client_etag: Option<Etag>) -> Result<Changed<MetaCratesResponse>> {
        self.crates
            .populate_if_uncached(client_etag, || async {
                let sandbox = AsyncSandbox::new().await.context(SandboxCreation)?;
                sandbox.crates().await.context(ListingCrates)
            })
            .await
            .map(|changed| changed.map(Into::into))
    }

    async fn version_stable(
        &self,
        client_etag: Option<Etag>,
    ) -> Result<Changed<MetaVersionResponse>> {
        self.version_stable
            .populate_if_uncached(client_etag, || async {
                let sandbox = AsyncSandbox::new().await.context(SandboxCreation)?;
                sandbox
                    .version(sandbox::Channel::Stable)
                    .await
                    .context(VersionStable)
            })
            .await
            .map(|changed| changed.map(Into::into))
    }

    async fn version_beta(
        &self,
        client_etag: Option<Etag>,
    ) -> Result<Changed<MetaVersionResponse>> {
        self.version_beta
            .populate_if_uncached(client_etag, || async {
                let sandbox = AsyncSandbox::new().await.context(SandboxCreation)?;
                sandbox
                    .version(sandbox::Channel::Beta)
                    .await
                    .context(VersionBeta)
            })
            .await
            .map(|changed| changed.map(Into::into))
    }

    async fn version_nightly(
        &self,
        client_etag: Option<Etag>,
    ) -> Result<Changed<MetaVersionResponse>> {
        self.version_nightly
            .populate_if_uncached(client_etag, || async {
                let sandbox = AsyncSandbox::new().await.context(SandboxCreation)?;
                sandbox
                    .version(sandbox::Channel::Nightly)
                    .await
                    .context(VersionNightly)
            })
            .await
            .map(|changed| changed.map(Into::into))
    }

    async fn version_rustfmt(
        &self,
        client_etag: Option<Etag>,
    ) -> Result<Changed<MetaVersionResponse>> {
        self.version_rustfmt
            .populate_if_uncached(client_etag, || async {
                let sandbox = AsyncSandbox::new().await.context(SandboxCreation)?;
                sandbox.version_rustfmt().await.context(VersionRustfmt)
            })
            .await
            .map(|changed| changed.map(Into::into))
    }

    async fn version_clippy(
        &self,
        client_etag: Option<Etag>,
    ) -> Result<Changed<MetaVersionResponse>> {
        self.version_clippy
            .populate_if_uncached(client_etag, || async {
                let sandbox = AsyncSandbox::new().await.context(SandboxCreation)?;
                sandbox.version_clippy().await.context(VersionClippy)
            })
            .await
            .map(|changed| changed.map(Into::into))
    }

    async fn version_miri(
        &self,
        client_etag: Option<Etag>,
    ) -> Result<Changed<MetaVersionResponse>> {
        self.version_miri
            .populate_if_uncached(client_etag, || async {
                let sandbox = AsyncSandbox::new().await.context(SandboxCreation)?;
                sandbox.version_miri().await.context(VersionMiri)
            })
            .await
            .map(|changed| changed.map(Into::into))
    }
}

#[derive(Debug)]
struct AsyncSandboxCacheOne<T>(Mutex<Option<CacheInfo<T>>>);

impl<T> Default for AsyncSandboxCacheOne<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<T> AsyncSandboxCacheOne<T>
where
    T: Hash + Clone,
{
    async fn populate_if_uncached<PopFutFn, PopFut>(
        &self,
        client_etag: Option<Etag>,
        f: PopFutFn,
    ) -> Result<Changed<T>>
    where
        PopFutFn: FnOnce() -> PopFut,
        PopFut: Future<Output = Result<T>>,
    {
        let mut cache = self.0.lock().await;

        *cache = cache
            .take()
            .filter(|cached| cached.at.elapsed() <= SANDBOX_CACHE_TIME_TO_LIVE);

        // `Option::get_or_insert_with`, but we need async and errors
        if cache.is_none() {
            let data = f().await?;
            let etag = simple_etag(&data);

            *cache = Some(CacheInfo {
                data,
                at: Instant::now(),
                etag,
            });
        };
        // `get_or_insert_with` uses unchecked_unreachable
        let cached = cache.as_mut().expect("Unreachable; we just set this");

        let matched = client_etag.map_or(false, |e| e == cached.etag);
        if matched {
            Ok(Changed::Unchanged)
        } else {
            Ok(Changed::Changed(cached.data.clone(), cached.etag.clone()))
        }
    }
}

struct CacheInfo<T> {
    data: T,
    at: Instant,
    etag: Etag,
}

fn simple_etag(data: impl Hash) -> String {
    let mut hasher = DefaultHasher::default();
    data.hash(&mut hasher);
    format!("{}", hasher.finish())
}

enum Changed<T> {
    Unchanged,
    Changed(T, Etag),
}

impl<T> Changed<T> {
    fn map<U>(self, f: impl FnOnce(T) -> U) -> Changed<U> {
        match self {
            Changed::Changed(v, e) => Changed::Changed(f(v), e),
            Changed::Unchanged => Changed::Unchanged,
        }
    }
}
