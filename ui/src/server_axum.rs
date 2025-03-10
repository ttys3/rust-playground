use crate::{
    gist,
    metrics::{
        track_metric_async, track_metric_force_endpoint_async, track_metric_no_request_async,
        Endpoint, GenerateLabels, SuccessDetails,
    },
    sandbox::{self, fut::Sandbox, Channel},
    CachingSnafu, ClippyRequest, ClippyResponse, CompilationSnafu, CompileRequest, CompileResponse,
    Config, Error, ErrorJson, EvaluateRequest, EvaluateResponse, EvaluationSnafu, ExecuteRequest,
    ExecuteResponse, ExecutionSnafu, ExpansionSnafu, FormatRequest, FormatResponse,
    FormattingSnafu, GhToken, GistCreationSnafu, GistLoadingSnafu, InterpretingSnafu, LintingSnafu,
    MacroExpansionRequest, MacroExpansionResponse, MetaCratesResponse, MetaGistCreateRequest,
    MetaGistResponse, MetaVersionResponse, MetricsToken, MiriRequest, MiriResponse, Result,
    SandboxCreationSnafu, ONE_HOUR, SANDBOX_CACHE_TIME_TO_LIVE,
};
use async_trait::async_trait;
use axum::{
    extract::{self, Extension, Path, TypedHeader},
    handler::Handler,
    headers::{authorization::Bearer, Authorization},
    http::{header, uri::PathAndQuery, HeaderValue, Method, Request, StatusCode, Uri},
    response::IntoResponse,
    routing::{get, get_service, post, MethodRouter},
    AddExtensionLayer, Router,
};
use axum_extra::middleware;
use futures::{future::BoxFuture, FutureExt};
use snafu::{prelude::*, IntoError};
use std::{
    convert::{TryFrom, TryInto},
    future::Future,
    mem, path,
    sync::Arc,
    time::Instant,
};
use tokio::sync::Mutex;
use tower_http::{
    cors::{self, CorsLayer},
    services::ServeDir,
    set_header::SetResponseHeader,
    trace::TraceLayer,
};

const MAX_AGE_ONE_DAY: HeaderValue = HeaderValue::from_static("public, max-age=86400");
const MAX_AGE_ONE_YEAR: HeaderValue = HeaderValue::from_static("public, max-age=31536000");

#[tokio::main]
pub(crate) async fn serve(config: Config) {
    let root_files = static_file_service(config.root_path(), MAX_AGE_ONE_DAY);
    let asset_files = static_file_service(config.asset_path(), MAX_AGE_ONE_YEAR);
    let rewrite_help_as_index = middleware::from_fn(rewrite_help_as_index);

    let mut app = Router::new()
        .fallback(root_files)
        .nest("/assets", asset_files)
        .layer(rewrite_help_as_index)
        .route("/evaluate.json", post(evaluate))
        .route("/compile", post(compile))
        .route("/execute", post(execute))
        .route("/format", post(format))
        .route("/clippy", post(clippy))
        .route("/miri", post(miri))
        .route("/macro-expansion", post(macro_expansion))
        .route("/meta/crates", get_or_post(meta_crates))
        .route("/meta/version/stable", get_or_post(meta_version_stable))
        .route("/meta/version/beta", get_or_post(meta_version_beta))
        .route("/meta/version/nightly", get_or_post(meta_version_nightly))
        .route("/meta/version/rustfmt", get_or_post(meta_version_rustfmt))
        .route("/meta/version/clippy", get_or_post(meta_version_clippy))
        .route("/meta/version/miri", get_or_post(meta_version_miri))
        .route("/meta/gist", post(meta_gist_create))
        .route("/meta/gist/:id", get(meta_gist_get))
        .route("/metrics", get(metrics))
        .layer(AddExtensionLayer::new(Arc::new(SandboxCache::default())))
        .layer(AddExtensionLayer::new(config.github_token()));

    if let Some(token) = config.metrics_token() {
        app = app.layer(AddExtensionLayer::new(token))
    }

    if config.use_cors() {
        app = app.layer({
            CorsLayer::new()
                .allow_origin(cors::any())
                .allow_headers([header::CONTENT_TYPE])
                .allow_methods([Method::GET, Method::POST])
                .allow_credentials(false)
                .max_age(ONE_HOUR)
        });
    }

    // Basic access logging
    app = app.layer(TraceLayer::new_for_http());

    axum::Server::bind(&config.server_socket_addr())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

fn get_or_post<T: 'static>(handler: impl Handler<T> + Copy) -> MethodRouter {
    get(handler).post(handler)
}

fn static_file_service(root: impl AsRef<path::Path>, max_age: HeaderValue) -> MethodRouter {
    let files = ServeDir::new(root).precompressed_gzip();

    let with_caching = SetResponseHeader::if_not_present(files, header::CACHE_CONTROL, max_age);

    get_service(with_caching).handle_error(|e| async move {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Unhandled internal error: {}", e),
        )
    })
}

async fn rewrite_help_as_index<B>(
    mut req: Request<B>,
    next: middleware::Next<B>,
) -> impl IntoResponse {
    let uri = req.uri_mut();
    if uri.path() == "/help" {
        let rewritten_uri = mem::take(uri);
        let mut parts = rewritten_uri.into_parts();
        parts.path_and_query = Some(PathAndQuery::from_static("/index.html"));
        *uri = Uri::from_parts(parts).unwrap();
    }
    next.run(req).await
}

// This is a backwards compatibilty shim. The Rust documentation uses
// this to run code in place.
async fn evaluate(Json(req): Json<EvaluateRequest>) -> Result<Json<EvaluateResponse>> {
    with_sandbox_force_endpoint(
        req,
        Endpoint::Evaluate,
        |sb, req| async move { sb.execute(req).await }.boxed(),
        EvaluationSnafu,
    )
    .await
    .map(Json)
}

async fn compile(Json(req): Json<CompileRequest>) -> Result<Json<CompileResponse>> {
    with_sandbox(
        req,
        |sb, req| async move { sb.compile(req).await }.boxed(),
        CompilationSnafu,
    )
    .await
    .map(Json)
}

async fn execute(Json(req): Json<ExecuteRequest>) -> Result<Json<ExecuteResponse>> {
    with_sandbox(
        req,
        |sb, req| async move { sb.execute(req).await }.boxed(),
        ExecutionSnafu,
    )
    .await
    .map(Json)
}

async fn format(Json(req): Json<FormatRequest>) -> Result<Json<FormatResponse>> {
    with_sandbox(
        req,
        |sb, req| async move { sb.format(req).await }.boxed(),
        FormattingSnafu,
    )
    .await
    .map(Json)
}

async fn clippy(Json(req): Json<ClippyRequest>) -> Result<Json<ClippyResponse>> {
    with_sandbox(
        req,
        |sb, req| async move { sb.clippy(req).await }.boxed(),
        LintingSnafu,
    )
    .await
    .map(Json)
}

async fn miri(Json(req): Json<MiriRequest>) -> Result<Json<MiriResponse>> {
    with_sandbox(
        req,
        |sb, req| async move { sb.miri(req).await }.boxed(),
        InterpretingSnafu,
    )
    .await
    .map(Json)
}

async fn macro_expansion(
    Json(req): Json<MacroExpansionRequest>,
) -> Result<Json<MacroExpansionResponse>> {
    with_sandbox(
        req,
        |sb, req| async move { sb.macro_expansion(req).await }.boxed(),
        ExpansionSnafu,
    )
    .await
    .map(Json)
}

async fn with_sandbox<F, Req, Resp, SbReq, SbResp, Ctx>(req: Req, f: F, ctx: Ctx) -> Result<Resp>
where
    for<'req> F: FnOnce(Sandbox, &'req SbReq) -> BoxFuture<'req, sandbox::Result<SbResp>>,
    Resp: From<SbResp>,
    SbReq: TryFrom<Req, Error = Error> + GenerateLabels,
    SbResp: SuccessDetails,
    Ctx: IntoError<Error, Source = sandbox::Error>,
{
    let sandbox = Sandbox::new().await.context(SandboxCreationSnafu)?;
    let request = req.try_into()?;
    track_metric_async(request, |request| f(sandbox, request))
        .await
        .map(Into::into)
        .context(ctx)
}

async fn with_sandbox_force_endpoint<F, Req, Resp, SbReq, SbResp, Ctx>(
    req: Req,
    endpoint: Endpoint,
    f: F,
    ctx: Ctx,
) -> Result<Resp>
where
    for<'req> F: FnOnce(Sandbox, &'req SbReq) -> BoxFuture<'req, sandbox::Result<SbResp>>,
    Resp: From<SbResp>,
    SbReq: TryFrom<Req, Error = Error> + GenerateLabels,
    SbResp: SuccessDetails,
    Ctx: IntoError<Error, Source = sandbox::Error>,
{
    let sandbox = Sandbox::new().await.context(SandboxCreationSnafu)?;
    let request = req.try_into()?;
    track_metric_force_endpoint_async(request, endpoint, |request| f(sandbox, request))
        .await
        .map(Into::into)
        .context(ctx)
}

async fn meta_crates(
    Extension(cache): Extension<Arc<SandboxCache>>,
) -> Result<Json<MetaCratesResponse>> {
    track_metric_no_request_async(Endpoint::MetaCrates, || cache.crates())
        .await
        .map(Json)
}

async fn meta_version_stable(
    Extension(cache): Extension<Arc<SandboxCache>>,
) -> Result<Json<MetaVersionResponse>> {
    track_metric_no_request_async(Endpoint::MetaVersionStable, || cache.version_stable())
        .await
        .map(Json)
}

async fn meta_version_beta(
    Extension(cache): Extension<Arc<SandboxCache>>,
) -> Result<Json<MetaVersionResponse>> {
    track_metric_no_request_async(Endpoint::MetaVersionBeta, || cache.version_beta())
        .await
        .map(Json)
}

async fn meta_version_nightly(
    Extension(cache): Extension<Arc<SandboxCache>>,
) -> Result<Json<MetaVersionResponse>> {
    track_metric_no_request_async(Endpoint::MetaVersionNightly, || cache.version_nightly())
        .await
        .map(Json)
}

async fn meta_version_rustfmt(
    Extension(cache): Extension<Arc<SandboxCache>>,
) -> Result<Json<MetaVersionResponse>> {
    track_metric_no_request_async(Endpoint::MetaVersionRustfmt, || cache.version_rustfmt())
        .await
        .map(Json)
}

async fn meta_version_clippy(
    Extension(cache): Extension<Arc<SandboxCache>>,
) -> Result<Json<MetaVersionResponse>> {
    track_metric_no_request_async(Endpoint::MetaVersionClippy, || cache.version_clippy())
        .await
        .map(Json)
}

async fn meta_version_miri(
    Extension(cache): Extension<Arc<SandboxCache>>,
) -> Result<Json<MetaVersionResponse>> {
    track_metric_no_request_async(Endpoint::MetaVersionMiri, || cache.version_miri())
        .await
        .map(Json)
}

async fn meta_gist_create(
    Extension(token): Extension<GhToken>,
    Json(req): Json<MetaGistCreateRequest>,
) -> Result<Json<MetaGistResponse>> {
    let token = String::clone(&token.0);
    gist::create_future(token, req.code)
        .await
        .map(Into::into)
        .map(Json)
        .context(GistCreationSnafu)
}

async fn meta_gist_get(
    Extension(token): Extension<GhToken>,
    Path(id): Path<String>,
) -> Result<Json<MetaGistResponse>> {
    let token = String::clone(&token.0);
    gist::load_future(token, &id)
        .await
        .map(Into::into)
        .map(Json)
        .context(GistLoadingSnafu)
}

async fn metrics(_: MetricsAuthorization) -> Result<Vec<u8>, StatusCode> {
    use prometheus::{Encoder, TextEncoder};

    let metric_families = prometheus::gather();
    let encoder = TextEncoder::new();
    let mut buffer = Vec::new();

    encoder
        .encode(&metric_families, &mut buffer)
        .map(|_| buffer)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Debug)]
struct MetricsAuthorization;

type MetricsAuthorizationRejection = (StatusCode, &'static str);

impl MetricsAuthorization {
    const FAILURE: MetricsAuthorizationRejection = (StatusCode::UNAUTHORIZED, "Wrong credentials");
}

#[async_trait]
impl<B> extract::FromRequest<B> for MetricsAuthorization
where
    B: Send,
{
    type Rejection = MetricsAuthorizationRejection;

    async fn from_request(req: &mut extract::RequestParts<B>) -> Result<Self, Self::Rejection> {
        match Extension::<MetricsToken>::from_request(req).await {
            Ok(Extension(expected)) => {
                match TypedHeader::<Authorization<Bearer>>::from_request(req).await {
                    Ok(TypedHeader(Authorization(actual))) => {
                        if actual.token() == &*expected.0 {
                            Ok(Self)
                        } else {
                            Err(Self::FAILURE)
                        }
                    }
                    Err(_) => Err(Self::FAILURE),
                }
            }
            // If we haven't set a code at all, allow the request.
            Err(_) => Ok(Self),
        }
    }
}

#[derive(Debug, Default)]
struct SandboxCache {
    // PERF: Since we clone these a lot, it could be more efficient to
    // change them to contain `Arc`s. e.g., `Vec<Foo>` could become
    // `Arc<[Foo]>`.
    crates: CacheOne<MetaCratesResponse>,
    version_stable: CacheOne<MetaVersionResponse>,
    version_beta: CacheOne<MetaVersionResponse>,
    version_nightly: CacheOne<MetaVersionResponse>,
    version_rustfmt: CacheOne<MetaVersionResponse>,
    version_clippy: CacheOne<MetaVersionResponse>,
    version_miri: CacheOne<MetaVersionResponse>,
}

impl SandboxCache {
    async fn crates(&self) -> Result<MetaCratesResponse> {
        self.crates
            .fetch(
                |sandbox| async move { Ok(sandbox.crates().await.context(CachingSnafu)?.into()) },
            )
            .await
    }

    async fn version_stable(&self) -> Result<MetaVersionResponse> {
        self.version_stable
            .fetch(|sandbox| async move {
                let version = sandbox
                    .version(Channel::Stable)
                    .await
                    .context(CachingSnafu)?;
                Ok(version.into())
            })
            .await
    }

    async fn version_beta(&self) -> Result<MetaVersionResponse> {
        self.version_beta
            .fetch(|sandbox| async move {
                let version = sandbox.version(Channel::Beta).await.context(CachingSnafu)?;
                Ok(version.into())
            })
            .await
    }

    async fn version_nightly(&self) -> Result<MetaVersionResponse> {
        self.version_nightly
            .fetch(|sandbox| async move {
                let version = sandbox
                    .version(Channel::Nightly)
                    .await
                    .context(CachingSnafu)?;
                Ok(version.into())
            })
            .await
    }

    async fn version_rustfmt(&self) -> Result<MetaVersionResponse> {
        self.version_rustfmt
            .fetch(|sandbox| async move {
                Ok(sandbox
                    .version_rustfmt()
                    .await
                    .context(CachingSnafu)?
                    .into())
            })
            .await
    }

    async fn version_clippy(&self) -> Result<MetaVersionResponse> {
        self.version_clippy
            .fetch(|sandbox| async move {
                Ok(sandbox.version_clippy().await.context(CachingSnafu)?.into())
            })
            .await
    }

    async fn version_miri(&self) -> Result<MetaVersionResponse> {
        self.version_miri
            .fetch(|sandbox| async move {
                Ok(sandbox.version_miri().await.context(CachingSnafu)?.into())
            })
            .await
    }
}

#[derive(Debug)]
struct CacheOne<T>(Mutex<Option<CacheInfo<T>>>);

impl<T> Default for CacheOne<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<T> CacheOne<T>
where
    T: Clone,
{
    async fn fetch<F, FFut>(&self, generator: F) -> Result<T>
    where
        F: FnOnce(Sandbox) -> FFut,
        FFut: Future<Output = Result<T>>,
    {
        let data = &mut *self.0.lock().await;
        match data {
            Some(info) => {
                if info.creation_time.elapsed() <= SANDBOX_CACHE_TIME_TO_LIVE {
                    Ok(info.value.clone())
                } else {
                    Self::set_value(data, generator).await
                }
            }
            None => Self::set_value(data, generator).await,
        }
    }

    async fn set_value<F, FFut>(data: &mut Option<CacheInfo<T>>, generator: F) -> Result<T>
    where
        F: FnOnce(Sandbox) -> FFut,
        FFut: Future<Output = Result<T>>,
    {
        let sandbox = Sandbox::new().await.context(SandboxCreationSnafu)?;
        let value = generator(sandbox).await?;

        *data = Some(CacheInfo::build(&value));

        Ok(value)
    }
}

#[derive(Debug)]
struct CacheInfo<T> {
    value: T,
    creation_time: Instant,
}

impl<T> CacheInfo<T> {
    fn build(value: &T) -> Self
    where
        T: Clone,
    {
        let value = value.clone();
        let creation_time = Instant::now();

        Self {
            value,
            creation_time,
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        Json(ErrorJson {
            error: self.to_string(),
        })
        .into_response()
    }
}

/// This type only exists so that we can recover from the `axum::Json`
/// error and format it using our expected JSON error object.
struct Json<T>(T);

#[async_trait]
impl<T, B> extract::FromRequest<B> for Json<T>
where
    T: serde::de::DeserializeOwned,
    B: axum::body::HttpBody + Send,
    B::Data: Send,
    B::Error: Into<axum::BoxError>,
{
    type Rejection = axum::response::Response;

    async fn from_request(req: &mut extract::RequestParts<B>) -> Result<Self, Self::Rejection> {
        match axum::Json::<T>::from_request(req).await {
            Ok(v) => Ok(Self(v.0)),
            Err(e) => {
                let error = format!("Unable to deserialize request: {e}");
                Err(axum::Json(ErrorJson { error }).into_response())
            }
        }
    }
}

impl<T> IntoResponse for Json<T>
where
    T: serde::Serialize,
{
    fn into_response(self) -> axum::response::Response {
        axum::Json(self.0).into_response()
    }
}
