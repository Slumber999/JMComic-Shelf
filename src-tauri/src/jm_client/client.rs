//! JmClient 的方法实现

use super::*;

/// 临时诊断：API 线路每次尝试的耗时写到 logcat（tag=JMAPI），adb 直接读；
/// 桌面端没有 logcat，同一份数据落 debug 日志，冷启动时间戳可直接查
fn api_measure(line: &str) {
    tracing::debug!("{line}");
    crate::android_log::log_line("JMAPI", line);
}

fn jm_now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default()
}

/// 签到接口没登录时的文案：/daily 不带 user_id、/daily_chk 没登录态都返回 `data: []`
const SIGN_NOT_LOGGED_IN: &str = "登录态无效或已过期，请重新登录";

/// 评论归属：官方 /forum 按 mode 区分——全站/漫画是 `mode=all`（漫画多带 aid），
/// 小说是 `mode=novel` + nid。注意带 `mode=all` 时官方会直接忽略 nid，只会回全站评论
///
/// 官方评论没有单独的章节参数，评论是按「章节（photo）」归档的，章节 id 就填在 aid 里；
/// 整本 id 恰好等于第 1 话的 id，所以「整本评论」实际只是第 1 话的评论
#[derive(Debug, Clone, Copy)]
enum CommentTarget {
    All,
    Comic(i64),
    Novel(i64),
}

impl CommentTarget {
    /// 拼官方接口的查询参数（page 是官方那套每页 10 条的页码）
    fn to_query(self, api_page: i64) -> serde_json::Value {
        let mut query = serde_json::Map::new();
        query.insert("page".to_string(), serde_json::Value::from(api_page));
        let mode = match self {
            CommentTarget::Novel(_) => "novel",
            CommentTarget::All | CommentTarget::Comic(_) => "all",
        };
        query.insert("mode".to_string(), serde_json::Value::from(mode));
        match self {
            CommentTarget::All => {}
            CommentTarget::Comic(aid) => {
                query.insert("aid".to_string(), serde_json::Value::from(aid));
            }
            CommentTarget::Novel(nid) => {
                query.insert("nid".to_string(), serde_json::Value::from(nid));
            }
        }
        serde_json::Value::Object(query)
    }
}

/// 选评论归属：章节 id 优先（官方只有 aid 一个字段，章节 id 也填这里），
/// 其次漫画 id，都不传就是全站最新评论
fn comment_target(aid: Option<i64>, chapter_id: Option<i64>) -> CommentTarget {
    match (aid, chapter_id) {
        (_, Some(chapter_id)) => CommentTarget::Comic(chapter_id),
        (Some(aid), None) => CommentTarget::Comic(aid),
        (None, None) => CommentTarget::All,
    }
}

/// 拼一次 API 请求（顺序发送与冷启动抢跑共用同一套 token / UA）
fn build_api_request(
    client: &ClientWithMiddleware,
    api_domain: &str,
    method: &reqwest::Method,
    path: ApiPath,
    query: Option<&serde_json::Value>,
    ts: u64,
) -> reqwest_middleware::RequestBuilder {
    let tokenparam = format!("{ts},{APP_VERSION}");
    let token = if path == ApiPath::GetScrambleId {
        utils::md5_hex(&format!("{ts}{APP_TOKEN_SECRET_2}"))
    } else {
        utils::md5_hex(&format!("{ts}{APP_TOKEN_SECRET}"))
    };
    client
        .request(method.clone(), format!("https://{api_domain}{}", path.as_str()).as_str())
        .header("token", token)
        .header("tokenparam", tokenparam)
        .header(
            "user-agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36",
        )
        .query(&query)
}

/// 快速失败的上限：从发出到出错超过这个时间，说明线路是慢/死的，
/// 重试只会再等一遍，不如直接换线
const FAST_FAIL_MS: u128 = 500;

/// 同线路快速重试前的间隔：给服务端的瞬时故障一点恢复时间
const FAST_RETRY_DELAY: Duration = Duration::from_millis(400);
/// 所有线路都失败后的最后一次同线路尝试（仅 GET、仅慢失败）
const LAST_RESORT_RETRY_DELAY: Duration = Duration::from_millis(600);

/// 有界快速重试的唯一判定：GET（幂等）+ 快速失败（耗时 < FAST_FAIL_MS）+ 本次请求还没重试过。
/// - POST 一律不重试：重复评论 / 签到 / 收藏比失败更糟
/// - 慢失败（例如 3 秒超时）说明这条线路是慢/死的，重试没有意义，交给换线逻辑
fn should_fast_retry(method: &reqwest::Method, elapsed_ms: u128, already_retried: bool) -> bool {
    method == reqwest::Method::GET && !already_retried && elapsed_ms < FAST_FAIL_MS
}

/// 一条线路上的一次尝试：成功、连接层失败、或「连上了但这次不能算成功」（5xx / 非 JSON）
enum Attempt {
    Ok {
        status: StatusCode,
        body: String,
    },
    /// 连接 / TLS / 超时 / 读 body 失败
    Transport(eyre::Report),
    /// 5xx，或该是 JSON 却不是 JSON：响应原样带着，最后一条线路要交回上层
    Unusable {
        reason: &'static str,
        status: StatusCode,
        body: String,
    },
}

impl Attempt {
    /// 失败原因（成功返回 None）：transport / http5xx / parse，也是重试日志的 reason 字段
    fn failure_reason(&self) -> Option<&'static str> {
        match self {
            Attempt::Ok { .. } => None,
            Attempt::Transport(_) => Some("transport"),
            Attempt::Unusable { reason, .. } => Some(*reason),
        }
    }

    /// 失败详情：只进日志与诊断，不进用户文案
    fn failure_detail(&self) -> String {
        match self {
            Attempt::Ok { .. } => String::new(),
            Attempt::Transport(err) => format!("{err}"),
            Attempt::Unusable { status, body, .. } => {
                let brief: String = body.chars().take(200).collect();
                format!("status {status} body={brief}")
            }
        }
    }
}

/// 记录一次同线路快速重试（zone=retry 便于事后统计瞬时故障）
/// - attempt = 2：这是这条线路上的第 2 次尝试，也是整个请求唯一允许的一次重试
fn log_fast_retry(domain: &str, elapsed_ms: u128, reason: &str, detail: &str) {
    tracing::warn!(
        zone = "retry",
        attempt = 2,
        elapsed_ms = elapsed_ms as u64,
        reason,
        domain = %domain,
        message = %detail,
        "API 请求快速失败，同线路再试一次"
    );
    api_measure(&format!(
        "api fast-retry domain={domain} ms={elapsed_ms} reason={reason}"
    ));
}

/// 记录一次换线前的线路失败（原来的日志格式，补上 reason 便于统计）
fn log_line_failed(domain: &str, elapsed_ms: u128, reason: &str, detail: &str) {
    tracing::warn!(
        domain = %domain,
        reason,
        elapsed_ms = elapsed_ms as u64,
        message = %detail,
        "API 线路失败，自动换下一条线路重试"
    );
    api_measure(&format!(
        "api fail domain={domain} ms={elapsed_ms} reason={reason} {detail}"
    ));
}

/// 冷启动抢跑：还没有已知可用线路时，候选线路并发发出同一个请求，
/// 谁先成功用谁（省掉「逐条线路等超时」的那几秒）
/// - `check_json` 为真时，body 解析不成 JmResp 也算这条线路没成功：抢跑不能把服务端的错误页当结果
async fn race_cold_start(
    client: Arc<RwLock<ClientWithMiddleware>>,
    candidates: Vec<String>,
    method: reqwest::Method,
    path: ApiPath,
    query: Option<serde_json::Value>,
    form: Option<serde_json::Value>,
    ts: u64,
    check_json: bool,
) -> Option<(String, StatusCode, String)> {
    let mut tasks = tokio::task::JoinSet::new();
    for domain in candidates {
        let client = client.clone();
        let method = method.clone();
        let query = query.clone();
        let form = form.clone();
        tasks.spawn(async move {
            let client = client.read().clone();
            let request = build_api_request(&client, &domain, &method, path, query.as_ref(), ts);
            let request = match &form {
                Some(payload) => request.form(&payload),
                None => request,
            };
            let response = match request.send().await {
                Ok(response) => response,
                Err(err) => return Err((domain, format!("{err}"))),
            };
            let status = response.status();
            if status.is_server_error() {
                return Err((domain, format!("status {status}")));
            }
            let body = match response.text().await {
                Ok(body) => body,
                Err(err) => return Err((domain, format!("读取响应失败: {err}"))),
            };
            if check_json
                && status == StatusCode::OK
                && serde_json::from_str::<JmResp>(&body).is_err()
            {
                return Err((domain, "响应不是接口数据".to_string()));
            }
            Ok((domain, status, body))
        });
    }
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(Ok((domain, status, body))) => {
                tasks.abort_all();
                return Some((domain, status, body));
            }
            Ok(Err((domain, message))) => {
                lines::mark_api_line_failed(&domain);
                api_measure(&format!("api race fail {domain} {message}"));
            }
            Err(err) => api_measure(&format!("api race join err {err}")),
        }
    }
    None
}
impl JmClient {
    /// 创建客户端。不会失败：代理配置不合法时退化为直连（并记错误日志），
    /// 保证应用永远能启动
    pub fn new(app: AppHandle) -> Self {
        let cookie_store = Arc::new(SessionCookieStore::default());
        let api_client = Arc::new(RwLock::new(create_api_client_or_direct(
            &app,
            &cookie_store,
        )));
        let img_client = Arc::new(RwLock::new(create_img_client_or_direct(&app)));

        Self {
            app,
            api_client,
            cookie_store,
            img_client,
            relogin_lock: Arc::new(tokio::sync::Mutex::new(())),
            session_generation: Arc::new(AtomicU64::new(0)),
            last_relogin_failure: Arc::new(RwLock::new(None)),
        }
    }

    /// 重新加载网络客户端（改了代理之后调用）
    /// - 两个客户端都建成功了才替换，避免出现"一半新一半旧"
    pub fn reload_client(&self) -> eyre::Result<()> {
        let api_client = create_api_client(&self.app, &self.cookie_store)?;
        let img_client = create_img_client(&self.app)?;

        *self.api_client.write() = api_client;
        *self.img_client.write() = img_client;
        Ok(())
    }

    /// 当前该用的 API 域名：运行时换过线就用换过的，否则用配置里选的
    fn api_domain(&self) -> String {
        let configured = self.app.get_config().read().get_api_domain();
        lines::active_api_domain(&configured)
    }

    /// 发请求并把 body 读出来；连接失败 / TLS 失败 / 超时 / 5xx 时按候选顺序自动换线（最多 3 条），
    /// 全部失败才报错。登录态按域名隔离，换到新线路后没登录态就用保存的账号重登。
    ///
    /// 换线之前允许一次「同线路快速重试」（见 should_fast_retry）：GET 且 500ms 内就失败时，
    /// 先在同一条线路上隔 400ms 再试一次，还失败才换线。慢失败直接换线，POST 从不重试。
    ///
    /// - expect_json：接口是否该返回 JSON。是的话，GET 的非 JSON body 也算本次尝试失败
    ///   （服务端这次回的是它自己的错误页，典型瞬时故障，重试一次往往就好了）。
    ///   POST 不做这个判定：不能因为响应看不懂就换线重发，那条请求可能已经提交成功了
    /// - 返回 (状态码, body)：各接口按自己的规则解释 body
    async fn jm_request(
        &self,
        method: reqwest::Method,
        path: ApiPath,
        query: Option<serde_json::Value>,
        form: Option<serde_json::Value>,
        ts: u64,
        expect_json: bool,
    ) -> eyre::Result<(StatusCode, String)> {
        let mut candidates = lines::api_line_candidates(&self.api_domain());
        let request_started = jm_now_ms();
        // 「同线路快速重试」整个请求只有一次预算，用完就只剩换线
        let mut retried = false;
        // 只有 GET 才把「不是 JSON」当成这次尝试失败：POST 重发可能造成重复提交
        let check_json = expect_json && method == reqwest::Method::GET;

        // 冷启动还不知道哪条线路能用：候选并发抢跑，避免「第一条挂掉的线路先等 2s 超时」。
        // 只有 GET 能抢跑：同一条 POST 并发发到多条线路会被服务端执行两次
        // （收藏被反切回来、重复签到就是这条路径），POST 仍然按顺序换线
        if !lines::has_active_api_line()
            && candidates.len() > 1
            && method == reqwest::Method::GET
        {
            if let Some((domain, status, body)) = race_cold_start(
                self.api_client.clone(),
                candidates.clone(),
                method.clone(),
                path,
                query.clone(),
                form.clone(),
                ts,
                check_json,
            )
            .await
            {
                lines::mark_api_line_ok_and_persist(&self.app, &domain);
                api_measure(&format!(
                    "api race ok domain={domain} total={}",
                    jm_now_ms() - request_started
                ));
                return Ok((status, body));
            }
            // 抢跑期间有线路被记成失败：重新取一次候选，失败的会被熔断窗口跳过
            candidates = lines::api_line_candidates(&self.api_domain());
        }

        'lines: for (index, domain) in candidates.iter().enumerate() {
            let generation = self.session_generation.load(Ordering::Relaxed);
            if index > 0 && !self.cookie_store.has_session(domain) {
                // 新域名上还没有登录态：走同一套重登逻辑（含冷却），没存密码就只做匿名请求
                self.relogin(generation, domain).await;
            }

            let last_candidate = index + 1 == candidates.len();
            let (status, body) = loop {
                // 用单调时钟量「从发出到出错」的耗时：这是判定快速失败的依据
                let attempt_started = Instant::now();
                let attempt = self
                    .attempt_on_domain(
                        domain,
                        &method,
                        path,
                        query.as_ref(),
                        form.as_ref(),
                        ts,
                        check_json,
                    )
                    .await;
                let elapsed_ms = attempt_started.elapsed().as_millis();

                if let Some(reason) = attempt.failure_reason() {
                    let detail = attempt.failure_detail();
                    // 快速失败：GET 且刚出错，服务端瞬时故障往往同线路再来一次就好
                    if should_fast_retry(&method, elapsed_ms, retried) {
                        retried = true;
                        log_fast_retry(domain, elapsed_ms, reason, &detail);
                        tokio::time::sleep(FAST_RETRY_DELAY).await;
                        continue;
                    }
                    // 最后一条线路也慢失败（多半是超时）：GET 再给它一次机会。
                    // 网络抖动时这一下往往就通了，用户不必看到报错。
                    if last_candidate && method == reqwest::Method::GET && !retried && elapsed_ms >= 500 {
                        retried = true;
                        log_fast_retry(domain, elapsed_ms, reason, &detail);
                        tokio::time::sleep(LAST_RESORT_RETRY_DELAY).await;
                        continue;
                    }
                    // 慢失败 / POST / 已经重试过：走原来的换线逻辑
                    lines::mark_api_line_failed(domain);
                    log_line_failed(domain, elapsed_ms, reason, &detail);
                    if last_candidate {
                        // 没有下一条线路可换了：连接层错误原样返回，5xx / 非 JSON 的响应
                        // 也原样交给上层，由它按状态码给出错误文案
                        return match attempt {
                            Attempt::Transport(err) => Err(err),
                            Attempt::Unusable { status, body, .. } => Ok((status, body)),
                            Attempt::Ok { .. } => unreachable!("失败分支不可能是 Attempt::Ok"),
                        };
                    }
                    continue 'lines;
                }

                let (status, body) = match attempt {
                    Attempt::Ok { status, body } => (status, body),
                    _ => unreachable!("成功分支只可能是 Attempt::Ok"),
                };
                // 这条线路能连通，记下来（并落盘，下次冷启动先用它）
                lines::mark_api_line_ok_and_persist(&self.app, domain);
                api_measure(&format!(
                    "api ok domain={domain} index={index} ms={elapsed_ms} total={}",
                    jm_now_ms() - request_started
                ));
                break (status, body);
            };

            if status == StatusCode::UNAUTHORIZED && self.relogin(generation, domain).await {
                return self
                    .send_and_read(domain, &method, path, query.as_ref(), form.as_ref(), ts)
                    .await;
            }

            return Ok((status, body));
        }

        Err(eyre!("API 线路全部失败"))
    }

    async fn send_on_domain(
        &self,
        api_domain: &str,
        method: &reqwest::Method,
        path: ApiPath,
        query: Option<&serde_json::Value>,
        form: Option<&serde_json::Value>,
        ts: u64,
    ) -> eyre::Result<reqwest::Response> {
        let request = build_api_request(&self.api_client.read(), api_domain, method, path, query, ts);

        let http_resp = match form {
            Some(payload) => request.query(&query).form(payload).send().await,
            None => request.query(&query).send().await,
        }
        .map_err(|e| {
            if e.is_timeout() {
                eyre::Report::from(e).wrap_err("连接超时，请使用代理或换条线路重试")
            } else {
                eyre::Report::from(e)
            }
        })?;

        Ok(http_resp)
    }

    /// 在指定线路上发一次请求并读出 body（不做重试 / 换线判定）
    async fn send_and_read(
        &self,
        api_domain: &str,
        method: &reqwest::Method,
        path: ApiPath,
        query: Option<&serde_json::Value>,
        form: Option<&serde_json::Value>,
        ts: u64,
    ) -> eyre::Result<(StatusCode, String)> {
        let http_resp = self
            .send_on_domain(api_domain, method, path, query, form, ts)
            .await?;
        let status = http_resp.status();
        let body = http_resp.text().await.wrap_err("读取响应内容失败")?;
        Ok((status, body))
    }

    /// 在一条线路上发一次请求：连接 + 读 body，JSON 接口再校验 body 能不能解析。
    /// 不重试、不换线，只把这次尝试的结果分好类交给 jm_request
    async fn attempt_on_domain(
        &self,
        api_domain: &str,
        method: &reqwest::Method,
        path: ApiPath,
        query: Option<&serde_json::Value>,
        form: Option<&serde_json::Value>,
        ts: u64,
        check_json: bool,
    ) -> Attempt {
        let (status, body) = match self
            .send_and_read(api_domain, method, path, query, form, ts)
            .await
        {
            Ok(response) => response,
            Err(err) => return Attempt::Transport(err),
        };
        if status.is_server_error() {
            return Attempt::Unusable {
                reason: "http5xx",
                status,
                body,
            };
        }
        if check_json && status == StatusCode::OK && serde_json::from_str::<JmResp>(&body).is_err()
        {
            // 服务端这次回的是它自己的错误页 / 纯文本：完整原文进日志，用户只看一句人话
            tracing::warn!(status = %status, body = %body, "接口返回的不是 JSON，按瞬时故障处理");
            return Attempt::Unusable {
                reason: "parse",
                status,
                body,
            };
        }
        Attempt::Ok { status, body }
    }

    /// 获取接口文本：GetScrambleId 返回的是 HTML 模板，不能按 JSON 校验
    async fn jm_get(
        &self,
        path: ApiPath,
        query: Option<serde_json::Value>,
        ts: u64,
    ) -> eyre::Result<(StatusCode, String)> {
        self.jm_request(reqwest::Method::GET, path, query, None, ts, false)
            .await
    }

    /// 各接口通用的响应处理：状态码 → JmResp → code → data → 解密，返回解密后的 JSON 文本
    /// - `err_title`：状态码 / code / data 失败时的文案前缀，各接口自己的说法
    /// - `unauthorized_err`：401 的专属文案；None 时 401 按普通状态码处理
    async fn request_decrypted_data(
        &self,
        method: reqwest::Method,
        path: ApiPath,
        query: Option<serde_json::Value>,
        form: Option<serde_json::Value>,
        err_title: &str,
        unauthorized_err: Option<&str>,
    ) -> eyre::Result<String> {
        self.request_decrypted_data_with(
            method,
            path,
            query,
            form,
            err_title,
            unauthorized_err,
            None,
        )
        .await
    }

    /// 同上，外加 `no_data_err`：data 不是加密字符串时的专属文案。
    /// 签到接口没登录/没数据时返回的是 `data: []`（HTTP 200），单独给个人话
    async fn request_decrypted_data_with(
        &self,
        method: reqwest::Method,
        path: ApiPath,
        query: Option<serde_json::Value>,
        form: Option<serde_json::Value>,
        err_title: &str,
        unauthorized_err: Option<&str>,
        no_data_err: Option<&str>,
    ) -> eyre::Result<String> {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        // 重试 / 换线都在 jm_request 里做完了，这里拿到的已经是最后一次尝试的响应
        let (status, body) = self.jm_request(method, path, query, form, ts, true).await?;
        if let Some(message) = unauthorized_err {
            if status == reqwest::StatusCode::UNAUTHORIZED {
                return Err(eyre!("{message}"));
            }
        }
        if status != reqwest::StatusCode::OK {
            // 服务端原文（可能是它自己的错误页 / SQL 报错）只进日志，不给用户看
            tracing::error!(err_title, status = %status, body = %body, "接口返回了预料之外的状态码");
            return Err(eyre!(
                "{err_title}，预料之外的状态码({status})，请稍后重试"
            ));
        }
        // 尝试将body解析为JmResp：走到这里说明 body 不是 JSON（JSON 校验在重试层已经做过），
        // 原文只进日志，用户只看一句人话
        let jm_resp = match serde_json::from_str::<JmResp>(&body) {
            Ok(jm_resp) => jm_resp,
            Err(err) => {
                tracing::error!(err_title, body = %body, reason = %err, "将body解析为JmResp失败");
                return Err(eyre!(
                    "{err_title}，服务端返回的不是接口数据（可能是临时故障），请稍后重试"
                ));
            }
        };
        // 检查JmResp的code字段
        if jm_resp.code != 200 {
            return Err(eyre!("{err_title}，预料之外的code: {jm_resp:?}"));
        }
        // 检查JmResp的data字段
        let Some(data) = jm_resp.data.as_str() else {
            if let Some(message) = no_data_err {
                return Err(eyre!("{message}"));
            }
            return Err(eyre!("{err_title}，data字段不是字符串: {jm_resp:?}"));
        };
        // 解密data字段
        decrypt_data(ts, data)
    }

    /// 在 request_decrypted_data 之后把解密后的 JSON 文本反序列化为 T
    /// - `parse_err_title`：反序列化失败的文案前缀，最终文案是 `{parse_err_title}失败: {data}`
    async fn request_decrypted<T: serde::de::DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: ApiPath,
        query: Option<serde_json::Value>,
        form: Option<serde_json::Value>,
        err_title: &str,
        parse_err_title: &str,
    ) -> eyre::Result<T> {
        let data = self
            .request_decrypted_data(method, path, query, form, err_title, None)
            .await?;
        // 尝试将解密后的数据解析为T
        serde_json::from_str::<T>(&data).wrap_err(format!("{parse_err_title}失败: {data}"))
    }

    /// 退出登录：丢掉登录态 AVS，之后的请求退回游客身份
    #[instrument(level = "error", skip_all)]
    pub fn logout(&self) {
        self.cookie_store.clear_session();
        self.session_generation.fetch_add(1, Ordering::Relaxed);
        *self.last_relogin_failure.write() = None;
    }

    /// 在当前线路登录；登录态按域名保存，换线后不会把旧域名的 AVS 带过去
    #[instrument(level = "error", skip_all)]
    pub async fn login(
        &self,
        username: &str,
        password: &str,
    ) -> eyre::Result<GetUserProfileRespData> {
        let domain = self.api_domain();
        self.login_on_domain(&domain, username, password).await
    }

    async fn login_on_domain(
        &self,
        domain: &str,
        username: &str,
        password: &str,
    ) -> eyre::Result<GetUserProfileRespData> {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let form = json!({
            "username": username,
            "password": password,
        });
        // 发送登录请求。这里绕开 jm_request：登录本身就回 401 时不能再触发重登
        let http_resp = self
            .send_on_domain(
                domain,
                &reqwest::Method::POST,
                ApiPath::Login,
                None,
                Some(&form),
                ts,
            )
            .await?;
        // 检查http响应状态码
        let status = http_resp.status();
        // 登录成功的响应头里带登录态 AVS，先把头读出来再读 body
        let session_avs = extract_session_avs(&http_resp);
        let body = http_resp.text().await?;
        if status != reqwest::StatusCode::OK {
            return Err(eyre!(
                "使用账号密码登录失败，预料之外的状态码({status}): {body}"
            ));
        }
        // 尝试将body解析为JmResp
        let jm_resp = serde_json::from_str::<JmResp>(&body)
            .wrap_err(format!("将body解析为JmResp失败: {body}"))?;
        // 检查JmResp的code字段
        if jm_resp.code != 200 {
            return Err(eyre!("使用账号密码登录失败，预料之外的code: {jm_resp:?}"));
        }
        // 检查JmResp的data字段
        let data = jm_resp.data.as_str().ok_or_eyre(format!(
            "使用账号密码登录失败，data字段不是字符串: {jm_resp:?}"
        ))?;
        // 解密data字段
        let data = decrypt_data(ts, data)?;
        // 尝试将解密后的data字段解析为GetUserProfileRespData
        let mut user_profile = serde_json::from_str::<GetUserProfileRespData>(&data).wrap_err(
            format!("将解密后的data字段解析为GetUserProfileRespData失败: {data}"),
        )?;
        user_profile.photo = format!("https://{IMAGE_DOMAIN}/media/users/{}", user_profile.photo);

        if let Some(avs) = session_avs {
            self.cookie_store.set_session(domain, avs);
        }
        self.session_generation.fetch_add(1, Ordering::Relaxed);
        *self.last_relogin_failure.write() = None;

        Ok(user_profile)
    }

    /// 登录态失效时用配置里的账号密码重新登录一次
    /// - 登录态按域名隔离，domain 是这次要登录的线路；换线重登和 401 重登共用同一套冷却
    /// - 返回 false 表示这次没登成（没存账号密码、冷却期内、或者登录又失败了）
    async fn relogin(&self, generation: u64, domain: &str) -> bool {
        // 等锁的这段时间里这个域名已经登好了，直接重试就行
        if self.cookie_store.has_session(domain)
            && self.session_generation.load(Ordering::Relaxed) != generation
        {
            return true;
        }
        if let Some(at) = *self.last_relogin_failure.read() {
            if at.elapsed() < RELOGIN_COOLDOWN {
                return false;
            }
        }

        let _guard = self.relogin_lock.lock().await;
        // 等锁期间别人可能刚登录失败过；拿到锁后再复查一次冷却，别接着再打一次登录接口
        if let Some(at) = *self.last_relogin_failure.read() {
            if at.elapsed() < RELOGIN_COOLDOWN {
                return false;
            }
        }
        if self.cookie_store.has_session(domain)
            && self.session_generation.load(Ordering::Relaxed) != generation
        {
            return true;
        }

        let (username, password) = {
            let config = self.app.get_config();
            let config = config.read();
            (config.username.clone(), config.password.clone())
        };
        if username.is_empty() || password.is_empty() {
            return false;
        }

        match self.login_on_domain(domain, &username, &password).await {
            Ok(_) => {
                tracing::info!(domain = %domain, "登录态已失效，已用保存的账号重新登录");
                true
            }
            Err(err) => {
                tracing::error!(message = %err, domain = %domain, "自动重新登录失败");
                *self.last_relogin_failure.write() = Some(Instant::now());
                false
            }
        }
    }

    #[instrument(level = "error", skip_all)]
    pub async fn get_user_profile(&self) -> eyre::Result<GetUserProfileRespData> {
        // 发送获取用户信息请求
        let data = self
            .request_decrypted_data(
                reqwest::Method::POST,
                ApiPath::GetUserProfile,
                None,
                None,
                "获取用户信息失败",
                Some("获取用户信息失败，Cookie无效或已过期，请重新登录"),
            )
            .await?;
        // 尝试将解密后的data字段解析为GetUserProfileRespData
        let mut user_profile = serde_json::from_str::<GetUserProfileRespData>(&data).wrap_err(
            format!("将解密后的data字段解析为GetUserProfileRespData失败: {data}"),
        )?;
        user_profile.photo = format!("https://{IMAGE_DOMAIN}/media/users/{}", user_profile.photo);

        Ok(user_profile)
    }

    /// 今日签到状态：/daily 只认 user_id，登录态靠 /login（用户信息）校验
    #[instrument(level = "error", skip_all)]
    pub async fn get_sign_status(&self) -> eyre::Result<SignStatus> {
        let uid = self.get_user_profile().await?.uid;
        self.fetch_sign_status(&uid).await
    }

    /// 执行签到：官方要 user_id + daily_id，daily_id 只能先问一次签到活动
    #[instrument(level = "error", skip_all)]
    pub async fn sign_in(&self) -> eyre::Result<SignInResult> {
        let uid = self.get_user_profile().await?.uid;
        let status = self.fetch_sign_status(&uid).await?;

        let data = self
            .request_decrypted_data_with(
                reqwest::Method::POST,
                ApiPath::DailyCheck,
                None,
                Some(json!({ "user_id": uid, "daily_id": status.daily_id })),
                "签到失败",
                Some(SIGN_NOT_LOGGED_IN),
                Some(SIGN_NOT_LOGGED_IN),
            )
            .await?;
        let mut result = serde_json::from_str::<SignInResult>(&data)
            .wrap_err(format!("将解密后的data字段解析为SignInResult失败: {data}"))?;
        result.daily_id = status.daily_id;
        result.already_signed = result.msg.contains("已签到过");
        Ok(result)
    }

    /// 问一次签到活动：record 里「今天」由 SignStatus 按日期自己算
    async fn fetch_sign_status(&self, uid: &str) -> eyre::Result<SignStatus> {
        let data = self
            .request_decrypted_data_with(
                reqwest::Method::GET,
                ApiPath::Daily,
                Some(json!({ "user_id": uid })),
                None,
                "获取签到状态失败",
                Some(SIGN_NOT_LOGGED_IN),
                Some(SIGN_NOT_LOGGED_IN),
            )
            .await?;
        let mut status = serde_json::from_str::<SignStatus>(&data)
            .wrap_err(format!("将解密后的data字段解析为SignStatus失败: {data}"))?;
        status.resolve_signed_today();
        Ok(status)
    }

    #[instrument(
        level = "error",
        skip_all,
        fields(keyword = keyword, page = page, sort = ?sort)
    )]
    pub async fn search(
        &self,
        keyword: &str,
        page: i64,
        sort: SearchSort,
        year: Option<i64>,
        month: Option<i64>,
    ) -> eyre::Result<SearchResp> {
        // y/m 是官方的年月筛选（0 表示不限）
        let query = json!({
            "main_tag": 0,
            "search_query": keyword,
            "page": page,
            "o": sort.as_str(),
            "y": year.unwrap_or(0),
            "m": month.unwrap_or(0),
        });
        // 发送搜索请求
        let data = self
            .request_decrypted_data(
                reqwest::Method::GET,
                ApiPath::Search,
                Some(query),
                None,
                "搜索失败",
                None,
            )
            .await?;
        // 尝试将解密后的数据解析为 RedirectRespData
        if let Ok(redirect_resp_data) = serde_json::from_str::<RedirectRespData>(&data) {
            let comic_resp_data = self.get_comic(redirect_resp_data.redirect_aid).await?;
            return Ok(SearchResp::ComicRespData(Box::new(comic_resp_data)));
        }
        // 尝试将解密后的data字段解析为 SearchRespData
        if let Ok(search_resp_data) = serde_json::from_str::<SearchRespData>(&data) {
            return Ok(SearchResp::SearchRespData(search_resp_data));
        }
        Err(eyre!(
            "将解密后的数据解析为SearchRespData或RedirectRespData失败: {data}"
        ))
    }

    #[instrument(
        level = "error",
        skip_all,
        fields(category = category, order = order, page = page)
    )]
    pub async fn get_ranking(
        &self,
        category: &str,
        order: &str,
        page: i64,
    ) -> eyre::Result<SearchRespData> {
        let query = json!({
            "c": category,
            "o": order,
            "page": page,
        });
        self.request_decrypted(
            reqwest::Method::GET,
            ApiPath::GetRanking,
            Some(query),
            None,
            "获取排行榜失败",
            "将解密后的数据解析为SearchRespData",
        )
        .await
    }

    /// 漫画评论：aid 传漫画 id 就是整本评论，不传就是全站最新评论
    /// （官方 App 接口 /forum，只读；发评论走的是网页端接口，App 接口不支持）
    ///
    /// chapter_id 传某一话的章节 id 时取这一话的评论：官方没有章节参数，
    /// 评论本身按章节（photo）归档，章节 id 直接当 aid 用（实测同一本第 2/3/4 话
    /// 的总数各不相同，且与整本的不同）。chapter_id 优先于 aid。
    ///
    /// 官方接口每页固定 10 条、且不接受页大小参数，这里并发拉 3 页合成 30 条一页
    #[instrument(level = "error", skip_all, fields(aid = ?aid, chapter_id = ?chapter_id, page = page))]
    pub async fn get_comments(
        &self,
        aid: Option<i64>,
        page: i64,
        chapter_id: Option<i64>,
    ) -> eyre::Result<CommentPage> {
        let target = comment_target(aid, chapter_id);
        self.get_comments_by_target(target, page).await
    }

    /// 小说评论：官方 /forum 要 mode=novel + nid=<小说id>，响应结构与漫画评论一致
    #[instrument(level = "error", skip_all, fields(nid = nid, page = page))]
    pub async fn get_novel_comments(&self, nid: i64, page: i64) -> eyre::Result<CommentPage> {
        self.get_comments_by_target(CommentTarget::Novel(nid), page).await
    }

    /// 漫画 / 小说 / 全站评论共用的取页逻辑（说明见 get_comments）
    async fn get_comments_by_target(
        &self,
        target: CommentTarget,
        page: i64,
    ) -> eyre::Result<CommentPage> {
        const API_PAGE_SIZE: i64 = 10;
        const PAGES_PER_REQUEST: i64 = COMMENT_PAGE_SIZE / API_PAGE_SIZE;

        let first_api_page = (page.max(1) - 1) * PAGES_PER_REQUEST + 1;

        let (first, second, third) = tokio::join!(
            self.fetch_comment_api_page(target, first_api_page),
            self.fetch_comment_api_page(target, first_api_page + 1),
            self.fetch_comment_api_page(target, first_api_page + 2),
        );

        // 第一页必须成功（total 也取自它）
        let mut head = first?;
        let total = head.total;
        let mut list = std::mem::take(&mut head.list);

        // 注意：请求超出最后一页时，官方会把同一页再返回一次，
        // 所以这里按 CID 去重（评论很少时，第 2、3 个官方页就是第 1 页的重复）
        let mut seen: std::collections::HashSet<String> =
            list.iter().map(|comment| comment.cid.clone()).collect();
        for extra in [second, third] {
            let Ok(mut extra_page) = extra else {
                continue;
            };
            for comment in extra_page.list.drain(..) {
                if seen.insert(comment.cid.clone()) {
                    list.push(comment);
                }
            }
        }

        // 一页最多 30 条
        list.truncate(COMMENT_PAGE_SIZE as usize);

        Ok(CommentPage { list, total })
    }

    /// 拉官方的一页评论（固定 10 条）
    #[instrument(level = "error", skip_all, fields(comment_target = ?target, api_page = api_page))]
    async fn fetch_comment_api_page(
        &self,
        target: CommentTarget,
        api_page: i64,
    ) -> eyre::Result<CommentPage> {
        let mut comment_page: CommentPage = self
            .request_decrypted(
                reqwest::Method::GET,
                ApiPath::GetComments,
                Some(target.to_query(api_page)),
                None,
                "获取评论失败",
                "将解密后的数据解析为CommentPage",
            )
            .await?;
        // 正文是 HTML，统一转成纯文本
        comment_page.strip_html();
        Ok(comment_page)
    }

    /// 浏览记录：page 从 1 开始，返回的是漫画元数据（与漫画列表元素同形）
    #[instrument(level = "error", skip_all, fields(page = page))]
    pub async fn get_watch_history(&self, page: i64) -> eyre::Result<WatchHistoryPage> {
        let query = json!({
            "page": page,
        });
        self.request_decrypted(
            reqwest::Method::GET,
            ApiPath::WatchList,
            Some(query),
            None,
            "获取浏览记录失败",
            "将解密后的数据解析为WatchHistoryPage",
        )
        .await
    }

    /// 删除一条浏览记录：表单 `id=<记录id>`，服务端只回 {"status":1,"msg":"刪除成功"}
    #[instrument(level = "error", skip_all, fields(id = id))]
    pub async fn delete_watch_history(&self, id: &str) -> eyre::Result<()> {
        let form = json!({
            "id": id,
        });
        let data = self
            .request_decrypted_data(
                reqwest::Method::POST,
                ApiPath::WatchList,
                None,
                Some(form),
                "删除浏览记录失败",
                None,
            )
            .await?;

        // status 不是 1 就是没删掉，服务端的说法在 msg 里
        let resp: WatchHistoryDeleteResp = serde_json::from_str(&data)
            .wrap_err(format!("将解密后的数据解析为WatchHistoryDeleteResp失败: {data}"))?;
        if resp.status != 1 {
            return Err(eyre!(
                "删除浏览记录失败，服务端返回 status={}: {}",
                resp.status,
                resp.msg
            ));
        }
        Ok(())
    }

    /// 官方通知列表：GET /notifications（需要登录态）
    /// - 官方可能返回裸数组，也可能返回 {total, list}，两种都接
    #[instrument(level = "error", skip_all)]
    pub async fn get_notifications(&self) -> eyre::Result<NotificationPage> {
        let data = self
            .request_decrypted_data_with(
                reqwest::Method::GET,
                ApiPath::Notifications,
                None,
                None,
                "获取通知失败",
                Some("获取通知失败，请先登录"),
                // 未登录时官方回 {"code":200,"data":[],"errorMsg":"請登入!"}
                Some("获取通知失败，请先登录"),
            )
            .await?;
        if let Ok(page) = serde_json::from_str::<NotificationPage>(&data) {
            return Ok(page);
        }
        let list = serde_json::from_str::<Vec<NotificationItem>>(&data)
            .wrap_err(format!("将解密后的数据解析为通知列表失败: {data}"))?;
        let total = i64::try_from(list.len()).unwrap_or(i64::MAX);
        Ok(NotificationPage { total, list })
    }

    /// 标记一条通知为已读：POST /notifications，表单 `id` + `read=1`
    /// - 成功时 data 解密出来是一句确认文案（"通知已读"），拿到 data 就算标记成功
    #[instrument(level = "error", skip_all, fields(id = id))]
    pub async fn mark_notification_read(&self, id: i64) -> eyre::Result<()> {
        let form = json!({
            "id": id,
            "read": 1,
        });
        self.request_decrypted_data(
            reqwest::Method::POST,
            ApiPath::Notifications,
            None,
            Some(form),
            "标记通知已读失败",
            Some("标记通知已读失败，请先登录"),
        )
        .await?;
        Ok(())
    }

    /// 小说列表：`o` 是排序（"mr" = 最近更新），`page` 从 1 开始
    #[instrument(level = "error", skip_all, fields(order = o, page = page))]
    pub async fn get_novel_list(&self, o: &str, page: i64) -> eyre::Result<NovelPage> {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let query = json!({
            "o": o,
            "page": page,
            "t": ts,
        });
        // 发送获取小说列表请求
        self.request_decrypted(
            reqwest::Method::GET,
            ApiPath::Novels,
            Some(query),
            None,
            "获取小说列表失败",
            "将解密后的数据解析为NovelPage",
        )
        .await
    }

    #[instrument(level = "error", skip_all, fields(nid = nid))]
    pub async fn get_novel_detail(&self, nid: &str) -> eyre::Result<NovelDetail> {
        let query = json!({"nid": nid});
        // 发送获取小说详情请求
        self.request_decrypted(
            reqwest::Method::GET,
            ApiPath::Novel,
            Some(query),
            None,
            "获取小说详情失败",
            "将解密后的数据解析为NovelDetail",
        )
        .await
    }

    /// 小说章节正文：`content` 是 HTML，原样交给前端
    #[instrument(level = "error", skip_all, fields(ncid = ncid, lang = lang))]
    pub async fn get_novel_chapter(&self, ncid: &str, lang: &str) -> eyre::Result<NovelChapter> {
        let query = json!({
            "ncid": ncid,
            "lang": lang,
        });
        // 发送获取小说章节请求
        self.request_decrypted(
            reqwest::Method::GET,
            ApiPath::NovelChapters,
            Some(query),
            None,
            "获取小说章节失败",
            "将解密后的数据解析为NovelChapter",
        )
        .await
    }

    #[instrument(level = "error", skip_all, fields(search_query = query))]
    pub async fn search_novels(&self, query: &str) -> eyre::Result<NovelPage> {
        let params = json!({"search_query": query});
        // 发送搜索小说请求
        self.request_decrypted(
            reqwest::Method::GET,
            ApiPath::SearchNovels,
            Some(params),
            None,
            "搜索小说失败",
            "将解密后的数据解析为NovelPage",
        )
        .await
    }

    #[instrument(level = "error", skip_all, fields(aid = aid))]
    pub async fn get_comic(&self, aid: i64) -> eyre::Result<GetComicRespData> {
        let query = json!({"id": aid,});
        // 发送获取漫画请求
        self.request_decrypted(
            reqwest::Method::GET,
            ApiPath::GetComic,
            Some(query),
            None,
            "获取漫画失败",
            "将解密后的data字段解析为GetComicRespData",
        )
        .await
    }

    #[instrument(level = "error", skip_all, fields(chapter_id = id))]
    pub async fn get_chapter(&self, id: i64) -> eyre::Result<GetChapterRespData> {
        let query = json!({"id": id,});
        // 发送获取章节请求
        self.request_decrypted(
            reqwest::Method::GET,
            ApiPath::GetChapter,
            Some(query),
            None,
            "获取章节失败",
            "将解密后的data字段解析为GetChapterRespData",
        )
        .await
    }

    #[instrument(level = "error", skip_all, fields(chapter_id = id))]
    pub async fn get_scramble_id(&self, id: i64) -> eyre::Result<i64> {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let query = json!({
            "id": id,
            "v": ts,
            "mode": "vertical",
            "page": 0,
            "app_img_shunt": 1,
            "express": "off",
        });
        // 发送获取scramble_id请求
        let (status, body) = self.jm_get(ApiPath::GetScrambleId, Some(query), ts).await?;
        // 检查http响应状态码
        if status != reqwest::StatusCode::OK {
            return Err(eyre!(
                "获取scramble_id失败，预料之外的状态码({status}): {body}"
            ));
        }
        // 从body中提取scramble_id，如果提取失败则使用默认值
        let scramble_id = body
            .split("var scramble_id = ")
            .nth(1)
            .and_then(|s| s.split(';').next())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(220_980);
        Ok(scramble_id)
    }

    #[instrument(
        level = "error",
        skip_all,
        fields(folder_id = folder_id, page = page, sort = ?sort)
    )]
    pub async fn get_favorite_folder(
        &self,
        folder_id: i64,
        page: i64,
        sort: FavoriteSort,
    ) -> eyre::Result<GetFavoriteRespData> {
        let query = json!({
            "page": page,
            "o": sort.as_str(),
            "folder_id": folder_id,
        });
        // 发送获取收藏夹请求
        self.request_decrypted(
            reqwest::Method::GET,
            ApiPath::GetFavoriteFolder,
            Some(query),
            None,
            "获取收藏夹失败",
            "将解密后的data字段解析为GetFavoriteRespData",
        )
        .await
    }

    #[instrument(level = "error", skip_all)]
    pub async fn get_weekly_info(&self) -> eyre::Result<GetWeeklyInfoRespData> {
        self.request_decrypted(
            reqwest::Method::GET,
            ApiPath::GetWeeklyInfo,
            None,
            None,
            "获取每周必看信息失败",
            "将解密后的data字段解析为GetWeeklyInfoRespData",
        )
        .await
    }

    #[instrument(
        level = "error",
        skip_all,
        fields(category_id = category_id, type_id = type_id)
    )]
    pub async fn get_weekly(
        &self,
        category_id: &str,
        type_id: &str,
    ) -> eyre::Result<GetWeeklyRespData> {
        let query = json!({
            "id": category_id,
            "type": type_id,
        });
        self.request_decrypted(
            reqwest::Method::GET,
            ApiPath::GetWeekly,
            Some(query),
            None,
            "获取每周必看信息失败",
            "将解密后的data字段解析为GetWeeklyRespData",
        )
        .await
    }

    /// 每周连载更新：type = all / manga / hanman，date = 0..6（周一..周日），page 从 1 开始
    #[instrument(level = "error", skip_all, fields(type_id = type_id, date = date, page = page))]
    pub async fn get_serialization(
        &self,
        type_id: &str,
        date: i64,
        page: i64,
    ) -> eyre::Result<GetWeeklyRespData> {
        let query = json!({
            "type": type_id,
            "date": date,
            "page": page,
        });
        self.request_decrypted(
            reqwest::Method::GET,
            ApiPath::GetSerialization,
            Some(query),
            None,
            "获取连载更新失败",
            "将解密后的data字段解析为GetWeeklyRespData",
        )
        .await
    }

    #[instrument(level = "error", skip_all, fields(aid = aid))]
    pub async fn toggle_favorite_comic(&self, aid: i64) -> eyre::Result<ToggleFavoriteRespData> {
        let form = json!({
            "aid": aid,
        });
        // 发送 收藏/取消收藏 请求
        self.request_decrypted(
            reqwest::Method::POST,
            ApiPath::GetFavoriteFolder,
            None,
            Some(form),
            "收藏/取消收藏 失败",
            "将解密后的data字段解析为ToggleFavoriteRespData",
        )
        .await
    }

    #[instrument(level = "error", skip_all, fields(aid = aid, folder_id = folder_id))]
    pub async fn move_favorite_to_folder(&self, aid: i64, folder_id: &str) -> eyre::Result<()> {
        let form = json!({
            "type": "move",
            "folder_id": folder_id,
            "aid": aid,
        });
        let data = self
            .request_decrypted_data(
                reqwest::Method::POST,
                ApiPath::ManageFavoriteFolder,
                None,
                Some(form),
                "移动收藏夹失败",
                None,
            )
            .await?;
        // 这个接口即使操作没生效也可能返回 code 200，要自己看 status：
        // 解析不出来（限流页、错误页）就是没成功，不能按默认值当成功放过去
        let action_resp_data: FavoriteFolderActionRespData = serde_json::from_str(&data)
            .wrap_err("移动收藏夹失败：响应解析失败")?;
        if !action_resp_data.status.is_empty() && action_resp_data.status != "ok" {
            return Err(eyre!(
                "移动收藏夹失败: {}",
                if action_resp_data.msg.is_empty() {
                    action_resp_data.status.clone()
                } else {
                    action_resp_data.msg.clone()
                }
            ));
        }
        Ok(())
    }

    /// 官方分类树 + 常用标签分组
    /// - categories: 主分类（含作品数）与其子分类
    /// - blocks: 官方常用标签，按主题分组（主题A漫 / 角色扮演 / 特殊PLAY / 其他）
    #[instrument(level = "error", skip_all)]
    pub async fn get_categories(&self) -> eyre::Result<CategoryResp> {
        let value: serde_json::Value = self
            .request_decrypted(
                reqwest::Method::GET,
                ApiPath::GetCategories,
                None,
                None,
                "获取分类失败",
                "将解密后的分类数据解析为JSON",
            )
            .await?;

        Ok(parse_categories(&value))
    }

    /// 下载图片：地址属于已知图片线路时，失败会自动换下一条线路重试（issue #195）
    #[instrument(level = "error", skip_all, fields(url = url))]
    pub async fn get_img_data_and_format(&self, url: &str) -> eyre::Result<(Bytes, ImageFormat)> {
        let Some(host) = url_host(url) else {
            return self.fetch_img_data_and_format_once(url).await;
        };
        // 不是图片线路的地址（自定义域名等）不做 fallback
        if !lines::is_image_line(host) {
            return self.fetch_img_data_and_format_once(url).await;
        }

        let candidates = lines::image_line_candidates(host);
        for domain in &candidates {
            let target = if *domain == host {
                url.to_string()
            } else {
                replace_url_host(url, domain)
            };
            match self.fetch_img_data_and_format_once(&target).await {
                Ok(result) => {
                    lines::mark_image_line_ok(domain);
                    return Ok(result);
                }
                Err(err) => {
                    // 每条线路的失败原因单独留一条 warn，聚合错误只给人话
                    tracing::warn!(
                        domain = *domain,
                        message = %err,
                        "图片线路失败，自动换下一条"
                    );
                    lines::mark_image_line_failed(domain);
                }
            }
        }

        // 每条线路的失败原因上面已经逐条 warn 过了，这里只给用户一句能照着做的话
        Err(eyre!(
            "图片线路全部失败（已尝试{}条）：请到设置里换一条图片线路，或检查代理是否正常",
            candidates.len()
        ))
    }

    #[instrument(level = "error", skip_all, fields(url = url))]
    async fn fetch_img_data_and_format_once(
        &self,
        url: &str,
    ) -> eyre::Result<(Bytes, ImageFormat)> {
        let request = self.img_client.read().get(url).header("user-agent", USER_AGENT);
        let mut image_data = read_image_body(request.send().await?).await?;

        if image_data.is_empty() {
            // 如果图片为空，说明jm那边缓存失效了，带上时间戳再次请求，以避免缓存
            let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
            let query = json!({"ts": ts});
            let request = self.img_client.read().get(url).query(&query);

            image_data = read_image_body(request.send().await?).await?;
        }

        let format = image::guess_format(&image_data)
            .wrap_err("无法从图片数据中猜测出图片格式，可能图片数据不完整或已损坏")?;

        Ok((image_data, format))
    }
}

/// 图片响应转字节：非 200 连正文一起报错（正文通常是 CDN 的错误页）
async fn read_image_body(http_resp: reqwest::Response) -> eyre::Result<Bytes> {
    let status = http_resp.status();
    if status != StatusCode::OK {
        let text = http_resp.text().await?;
        return Err(eyre!("下载图片失败，预料之外的状态码({status}): {text}"));
    }
    Ok(http_resp.bytes().await?)
}

#[cfg(test)]
mod retry_tests {
    use super::*;

    #[test]
    fn fast_get_failure_should_retry_once() {
        // 连接被拒 / 立即 5xx / 非 JSON：都在 500ms 内失败，GET 可以同线路再试一次
        assert!(should_fast_retry(&reqwest::Method::GET, 0, false));
        assert!(should_fast_retry(&reqwest::Method::GET, 499, false));
    }

    #[test]
    fn slow_post_and_retried_should_not_retry() {
        // 500ms 及以上说明线路是慢/死的：直接换线，重试只会再等一遍
        assert!(!should_fast_retry(&reqwest::Method::GET, 500, false));
        assert!(!should_fast_retry(&reqwest::Method::GET, 3_000, false));
        // POST 不幂等：评论 / 签到 / 收藏绝不能重放
        assert!(!should_fast_retry(&reqwest::Method::POST, 10, false));
        // 一个请求只重试一次
        assert!(!should_fast_retry(&reqwest::Method::GET, 10, true));
    }
}

#[cfg(test)]
mod comment_tests {
    use super::*;

    /// 官方没有章节参数：章节 id 必须落到 aid 上，且 chapter_id 优先于 aid
    #[test]
    fn chapter_comment_uses_chapter_id_as_aid() {
        assert!(matches!(
            comment_target(Some(1044827), Some(1044832)),
            CommentTarget::Comic(1044832)
        ));
        assert!(matches!(
            comment_target(Some(1044827), None),
            CommentTarget::Comic(1044827)
        ));
        assert!(matches!(comment_target(None, None), CommentTarget::All));

        let query = CommentTarget::Comic(1044832).to_query(2);
        assert_eq!(query["mode"], "all");
        assert_eq!(query["aid"], 1044832);
        assert_eq!(query["page"], 2);
    }
}
