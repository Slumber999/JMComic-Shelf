//! JmClient 的方法实现

use super::*;

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

    /// 发请求；需要登录的接口回 401 就用保存的账号重新登录一次，然后重试
    async fn jm_request(
        &self,
        method: reqwest::Method,
        path: ApiPath,
        query: Option<serde_json::Value>,
        form: Option<serde_json::Value>,
        ts: u64,
    ) -> eyre::Result<reqwest::Response> {
        let generation = self.session_generation.load(Ordering::Relaxed);
        let http_resp = self
            .send(&method, path, query.as_ref(), form.as_ref(), ts)
            .await?;

        if http_resp.status() != StatusCode::UNAUTHORIZED || !self.relogin(generation).await {
            return Ok(http_resp);
        }

        self.send(&method, path, query.as_ref(), form.as_ref(), ts)
            .await
    }

    async fn send(
        &self,
        method: &reqwest::Method,
        path: ApiPath,
        query: Option<&serde_json::Value>,
        form: Option<&serde_json::Value>,
        ts: u64,
    ) -> eyre::Result<reqwest::Response> {
        let tokenparam = format!("{ts},{APP_VERSION}");
        let token = if path == ApiPath::GetScrambleId {
            utils::md5_hex(&format!("{ts}{APP_TOKEN_SECRET_2}"))
        } else {
            utils::md5_hex(&format!("{ts}{APP_TOKEN_SECRET}"))
        };

        let api_domain = self.app.get_config().read().get_api_domain();
        let path = path.as_str();
        let request = self
            .api_client
            .read()
            .request(method.clone(), format!("https://{api_domain}{path}").as_str())
            .header("token", token)
            .header("tokenparam", tokenparam)
            .header("user-agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36");

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

    async fn jm_get(
        &self,
        path: ApiPath,
        query: Option<serde_json::Value>,
        ts: u64,
    ) -> eyre::Result<reqwest::Response> {
        self.jm_request(reqwest::Method::GET, path, query, None, ts)
            .await
    }

    async fn jm_post(
        &self,
        path: ApiPath,
        query: Option<serde_json::Value>,
        payload: Option<serde_json::Value>,
        ts: u64,
    ) -> eyre::Result<reqwest::Response> {
        self.jm_request(reqwest::Method::POST, path, query, payload, ts)
            .await
    }

    /// 退出登录：丢掉登录态 AVS，之后的请求退回游客身份
    #[instrument(level = "error", skip_all)]
    pub fn logout(&self) {
        self.cookie_store.clear_session();
        self.session_generation.fetch_add(1, Ordering::Relaxed);
        *self.last_relogin_failure.write() = None;
    }

    #[instrument(level = "error", skip_all)]
    pub async fn login(
        &self,
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
            .send(&reqwest::Method::POST, ApiPath::Login, None, Some(&form), ts)
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
            self.cookie_store.set_session(avs);
        }
        self.session_generation.fetch_add(1, Ordering::Relaxed);
        *self.last_relogin_failure.write() = None;

        Ok(user_profile)
    }

    /// 登录态失效时用配置里的账号密码重新登录一次
    /// - 返回 false 表示这次没登成（没存账号密码、冷却期内、或者登录又失败了）
    async fn relogin(&self, generation: u64) -> bool {
        // 等锁的这段时间里别的请求已经登好了，直接重试就行
        if self.session_generation.load(Ordering::Relaxed) != generation {
            return true;
        }
        if let Some(at) = *self.last_relogin_failure.read() {
            if at.elapsed() < RELOGIN_COOLDOWN {
                return false;
            }
        }

        let _guard = self.relogin_lock.lock().await;
        if self.session_generation.load(Ordering::Relaxed) != generation {
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

        match self.login(&username, &password).await {
            Ok(_) => {
                tracing::info!("登录态已失效，已用保存的账号重新登录");
                true
            }
            Err(err) => {
                tracing::error!(message = %err, "自动重新登录失败");
                *self.last_relogin_failure.write() = Some(Instant::now());
                false
            }
        }
    }

    #[instrument(level = "error", skip_all)]
    pub async fn get_user_profile(&self) -> eyre::Result<GetUserProfileRespData> {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        // 发送获取用户信息请求
        let http_resp = self
            .jm_post(ApiPath::GetUserProfile, None, None, ts)
            .await?;
        // 检查http响应状态码
        let status = http_resp.status();
        let body = http_resp.text().await?;
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(eyre!("获取用户信息失败，Cookie无效或已过期，请重新登录"));
        } else if status != reqwest::StatusCode::OK {
            return Err(eyre!(
                "获取用户信息失败，预料之外的状态码({status}): {body}"
            ));
        }
        // 尝试将body解析为JmResp
        let jm_resp = serde_json::from_str::<JmResp>(&body)
            .wrap_err(format!("将body解析为JmResp失败: {body}"))?;
        // 检查JmResp的code字段
        if jm_resp.code != 200 {
            return Err(eyre!("获取用户信息失败，预料之外的code: {jm_resp:?}"));
        }
        // 检查JmResp的data字段
        let data = jm_resp
            .data
            .as_str()
            .ok_or_eyre(format!("获取用户信息失败，data字段不是字符串: {jm_resp:?}"))?;
        // 解密data字段
        let data = decrypt_data(ts, data)?;
        // 尝试将解密后的data字段解析为GetUserProfileRespData
        let mut user_profile = serde_json::from_str::<GetUserProfileRespData>(&data).wrap_err(
            format!("将解密后的data字段解析为GetUserProfileRespData失败: {data}"),
        )?;
        user_profile.photo = format!("https://{IMAGE_DOMAIN}/media/users/{}", user_profile.photo);

        Ok(user_profile)
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
        let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        // 发送搜索请求
        let http_resp = self.jm_get(ApiPath::Search, Some(query), ts).await?;
        // 检查http响应状态码
        let status = http_resp.status();
        let body = http_resp.text().await?;
        if status != reqwest::StatusCode::OK {
            return Err(eyre!("搜索失败，预料之外的状态码({status}): {body}"));
        }
        // 尝试将body解析为JmResp
        let jm_resp = serde_json::from_str::<JmResp>(&body)
            .wrap_err(format!("将body解析为JmResp失败: {body}"))?;
        // 检查JmResp的code字段
        if jm_resp.code != 200 {
            return Err(eyre!("搜索失败，预料之外的code: {jm_resp:?}"));
        }
        // 检查JmResp的data字段
        let data = jm_resp
            .data
            .as_str()
            .ok_or_eyre(format!("搜索失败，data字段不是字符串: {jm_resp:?}"))?;
        // 解密data字段
        let data = decrypt_data(ts, data)?;
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
        let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let http_resp = self.jm_get(ApiPath::GetRanking, Some(query), ts).await?;
        let status = http_resp.status();
        let body = http_resp.text().await?;
        if status != reqwest::StatusCode::OK {
            return Err(eyre!("获取排行榜失败，预料之外的状态码({status}): {body}"));
        }
        let jm_resp = serde_json::from_str::<JmResp>(&body)
            .wrap_err(format!("将body解析为JmResp失败: {body}"))?;
        if jm_resp.code != 200 {
            return Err(eyre!("获取排行榜失败，预料之外的code: {jm_resp:?}"));
        }
        let data = jm_resp
            .data
            .as_str()
            .ok_or_eyre(format!("获取排行榜失败，data字段不是字符串: {jm_resp:?}"))?;
        let data = decrypt_data(ts, data)?;
        let ranking_resp_data = serde_json::from_str::<SearchRespData>(&data)
            .wrap_err(format!("将解密后的数据解析为SearchRespData失败: {data}"))?;
        Ok(ranking_resp_data)
    }

    /// 漫画评论：aid 传漫画 id 就是单本评论，不传就是全站最新评论
    /// （官方 App 接口 /forum，只读；发评论走的是网页端接口，App 接口不支持）
    ///
    /// 官方接口每页固定 10 条、且不接受页大小参数，这里并发拉 3 页合成 30 条一页
    #[instrument(level = "error", skip_all, fields(aid = ?aid, page = page))]
    pub async fn get_comments(&self, aid: Option<i64>, page: i64) -> eyre::Result<CommentPage> {
        const API_PAGE_SIZE: i64 = 10;
        const PAGES_PER_REQUEST: i64 = COMMENT_PAGE_SIZE / API_PAGE_SIZE;

        let first_api_page = (page.max(1) - 1) * PAGES_PER_REQUEST + 1;

        let (first, second, third) = tokio::join!(
            self.fetch_comment_api_page(aid, first_api_page),
            self.fetch_comment_api_page(aid, first_api_page + 1),
            self.fetch_comment_api_page(aid, first_api_page + 2),
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
    #[instrument(level = "error", skip_all, fields(aid = ?aid, api_page = api_page))]
    async fn fetch_comment_api_page(
        &self,
        aid: Option<i64>,
        api_page: i64,
    ) -> eyre::Result<CommentPage> {
        let mut query = serde_json::Map::new();
        query.insert("mode".to_string(), serde_json::Value::String("all".to_string()));
        query.insert("page".to_string(), serde_json::Value::from(api_page));
        if let Some(aid) = aid {
            query.insert("aid".to_string(), serde_json::Value::from(aid));
        }

        let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let http_resp = self
            .jm_get(ApiPath::GetComments, Some(serde_json::Value::Object(query)), ts)
            .await?;

        let status = http_resp.status();
        let body = http_resp.text().await?;
        if status != reqwest::StatusCode::OK {
            return Err(eyre!("获取评论失败，预料之外的状态码({status}): {body}"));
        }

        let jm_resp =
            serde_json::from_str::<JmResp>(&body).wrap_err(format!("将body解析为JmResp失败: {body}"))?;
        if jm_resp.code != 200 {
            return Err(eyre!("获取评论失败，预料之外的code: {jm_resp:?}"));
        }

        let data = jm_resp
            .data
            .as_str()
            .ok_or_eyre(format!("获取评论失败，data字段不是字符串: {jm_resp:?}"))?;
        let data = decrypt_data(ts, data)?;

        let mut comment_page = serde_json::from_str::<CommentPage>(&data)
            .wrap_err(format!("将解密后的数据解析为CommentPage失败: {data}"))?;
        // 正文是 HTML，统一转成纯文本
        comment_page.strip_html();
        Ok(comment_page)
    }

    #[instrument(level = "error", skip_all, fields(aid = aid))]
    pub async fn get_comic(&self, aid: i64) -> eyre::Result<GetComicRespData> {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let query = json!({"id": aid,});
        // 发送获取漫画请求
        let http_resp = self.jm_get(ApiPath::GetComic, Some(query), ts).await?;
        // 检查http响应状态码
        let status = http_resp.status();
        let body = http_resp.text().await?;
        if status != reqwest::StatusCode::OK {
            return Err(eyre!("获取漫画失败，预料之外的状态码({status}): {body}"));
        }
        // 尝试将body解析为JmResp
        let jm_resp = serde_json::from_str::<JmResp>(&body)
            .wrap_err(format!("将body解析为JmResp失败: {body}"))?;
        // 检查JmResp的code字段
        if jm_resp.code != 200 {
            return Err(eyre!("获取漫画失败，预料之外的code: {jm_resp:?}"));
        }
        // 检查JmResp的data字段
        let data = jm_resp
            .data
            .as_str()
            .ok_or_eyre(format!("获取漫画失败，data字段不是字符串: {jm_resp:?}"))?;
        // 解密data字段
        let data = decrypt_data(ts, data)?;
        // 尝试将解密后的data字段解析为GetComicRespData
        let comic = serde_json::from_str::<GetComicRespData>(&data).wrap_err(format!(
            "将解密后的data字段解析为GetComicRespData失败: {data}"
        ))?;
        Ok(comic)
    }

    #[instrument(level = "error", skip_all, fields(chapter_id = id))]
    pub async fn get_chapter(&self, id: i64) -> eyre::Result<GetChapterRespData> {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let query = json!({"id": id,});
        // 发送获取章节请求
        let http_resp = self.jm_get(ApiPath::GetChapter, Some(query), ts).await?;
        // 检查http响应状态码
        let status = http_resp.status();
        let body = http_resp.text().await?;
        if status != reqwest::StatusCode::OK {
            return Err(eyre!("获取章节失败，预料之外的状态码({status}): {body}"));
        }
        // 尝试将body解析为JmResp
        let jm_resp = serde_json::from_str::<JmResp>(&body)
            .wrap_err(format!("将body解析为JmResp失败: {body}"))?;
        // 检查JmResp的code字段
        if jm_resp.code != 200 {
            return Err(eyre!("获取章节失败，预料之外的code: {jm_resp:?}"));
        }
        // 检查JmResp的data字段
        let data = jm_resp
            .data
            .as_str()
            .ok_or_eyre(format!("获取章节失败，data字段不是字符串: {jm_resp:?}"))?;
        // 解密data字段
        let data = decrypt_data(ts, data)?;
        // 尝试将解密后的data字段解析为GetChapterRespData
        let chapter = serde_json::from_str::<GetChapterRespData>(&data).wrap_err(format!(
            "将解密后的data字段解析为GetChapterRespData失败: {data}"
        ))?;
        Ok(chapter)
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
        let http_resp = self.jm_get(ApiPath::GetScrambleId, Some(query), ts).await?;
        // 检查http响应状态码
        let status = http_resp.status();
        let body = http_resp.text().await?;
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
        let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let query = json!({
            "page": page,
            "o": sort.as_str(),
            "folder_id": folder_id,
        });
        // 发送获取收藏夹请求
        let http_resp = self
            .jm_get(ApiPath::GetFavoriteFolder, Some(query), ts)
            .await?;
        // 检查http响应状态码
        let status = http_resp.status();
        let body = http_resp.text().await?;
        if status != reqwest::StatusCode::OK {
            return Err(eyre!("获取收藏夹失败，预料之外的状态码({status}): {body}"));
        }
        // 尝试将body解析为JmResp
        let jm_resp = serde_json::from_str::<JmResp>(&body)
            .wrap_err(format!("将body解析为JmResp失败: {body}"))?;
        // 检查JmResp的code字段
        if jm_resp.code != 200 {
            return Err(eyre!("获取收藏夹失败，预料之外的code: {jm_resp:?}"));
        }
        // 检查JmResp的data字段
        let data = jm_resp
            .data
            .as_str()
            .ok_or_eyre(format!("获取收藏夹失败，data字段不是字符串: {jm_resp:?}"))?;
        // 解密data字段
        let data = decrypt_data(ts, data)?;
        // 尝试将解密后的data字段解析为GetFavoriteRespData
        let favorite = serde_json::from_str::<GetFavoriteRespData>(&data).wrap_err(format!(
            "将解密后的data字段解析为GetFavoriteRespData失败: {data}"
        ))?;
        Ok(favorite)
    }

    #[instrument(level = "error", skip_all)]
    pub async fn get_weekly_info(&self) -> eyre::Result<GetWeeklyInfoRespData> {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let http_resp = self.jm_get(ApiPath::GetWeeklyInfo, None, ts).await?;
        // 检查http响应状态码
        let status = http_resp.status();
        let body = http_resp.text().await?;
        if status != reqwest::StatusCode::OK {
            return Err(eyre!(
                "获取每周必看信息失败，预料之外的状态码({status}): {body}"
            ));
        }
        // 尝试将body解析为JmResp
        let jm_resp = serde_json::from_str::<JmResp>(&body)
            .wrap_err(format!("将body解析为JmResp失败: {body}"))?;
        // 检查JmResp的code字段
        if jm_resp.code != 200 {
            return Err(eyre!("获取每周必看信息失败，预料之外的code: {jm_resp:?}"));
        }
        // 检查JmResp的data字段
        let data = jm_resp.data.as_str().ok_or_eyre(format!(
            "获取每周必看信息失败，data字段不是字符串: {jm_resp:?}"
        ))?;
        // 解密data字段
        let data = decrypt_data(ts, data)?;
        // 尝试将解密后的data字段解析为GetWeeklyInfoRespData
        let weekly_info = serde_json::from_str::<GetWeeklyInfoRespData>(&data).wrap_err(
            format!("将解密后的data字段解析为GetWeeklyInfoRespData失败: {data}"),
        )?;
        Ok(weekly_info)
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
        let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let query = json!({
            "id": category_id,
            "type": type_id,
        });
        let http_resp = self.jm_get(ApiPath::GetWeekly, Some(query), ts).await?;
        // 检查http响应状态码
        let status = http_resp.status();
        let body = http_resp.text().await?;
        if status != reqwest::StatusCode::OK {
            return Err(eyre!(
                "获取每周必看信息失败，预料之外的状态码({status}): {body}"
            ));
        }
        // 尝试将body解析为JmResp
        let jm_resp = serde_json::from_str::<JmResp>(&body)
            .wrap_err(format!("将body解析为JmResp失败: {body}"))?;
        // 检查JmResp的code字段
        if jm_resp.code != 200 {
            return Err(eyre!("获取每周必看信息失败，预料之外的code: {jm_resp:?}"));
        }
        // 检查JmResp的data字段
        let data = jm_resp.data.as_str().ok_or_eyre(format!(
            "获取每周必看信息失败，data字段不是字符串: {jm_resp:?}"
        ))?;
        // 解密data字段
        let data = decrypt_data(ts, data)?;
        // 尝试将解密后的data字段解析为GetWeeklyRespData
        let get_weekly_resp_data = serde_json::from_str::<GetWeeklyRespData>(&data).wrap_err(
            format!("将解密后的data字段解析为GetWeeklyRespData失败: {data}"),
        )?;
        Ok(get_weekly_resp_data)
    }

    #[instrument(level = "error", skip_all, fields(aid = aid))]
    pub async fn toggle_favorite_comic(&self, aid: i64) -> eyre::Result<ToggleFavoriteRespData> {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let form = json!({
            "aid": aid,
        });
        // 发送 收藏/取消收藏 请求
        let http_resp = self
            .jm_post(ApiPath::GetFavoriteFolder, None, Some(form), ts)
            .await?;
        // 检查http响应状态码
        let status = http_resp.status();
        let body = http_resp.text().await?;
        if status != reqwest::StatusCode::OK {
            return Err(eyre!(
                "收藏/取消收藏 失败，预料之外的状态码({status}): {body}"
            ));
        }
        // 尝试将body解析为JmResp
        let jm_resp = serde_json::from_str::<JmResp>(&body)
            .wrap_err(format!("将body解析为JmResp失败: {body}"))?;
        // 检查JmResp的code字段
        if jm_resp.code != 200 {
            return Err(eyre!("收藏/取消收藏 失败，预料之外的code: {jm_resp:?}"));
        }
        // 检查JmResp的data字段
        let data = jm_resp.data.as_str().ok_or_eyre(format!(
            "收藏/取消收藏 失败，data字段不是字符串: {jm_resp:?}"
        ))?;
        // 解密data字段
        let data = decrypt_data(ts, data)?;
        // 尝试将解密后的data字段解析为ToggleFavoriteRespData
        let toggle_favorite_resp_data = serde_json::from_str::<ToggleFavoriteRespData>(&data)
            .wrap_err(format!(
                "将解密后的data字段解析为ToggleFavoriteRespData失败: {data}"
            ))?;
        Ok(toggle_favorite_resp_data)
    }

    #[instrument(level = "error", skip_all, fields(aid = aid, folder_id = folder_id))]
    pub async fn move_favorite_to_folder(&self, aid: i64, folder_id: &str) -> eyre::Result<()> {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let form = json!({
            "type": "move",
            "folder_id": folder_id,
            "aid": aid,
        });
        let http_resp = self
            .jm_post(ApiPath::ManageFavoriteFolder, None, Some(form), ts)
            .await?;
        let status = http_resp.status();
        let body = http_resp.text().await?;
        if status != reqwest::StatusCode::OK {
            return Err(eyre!("移动收藏夹失败，预料之外的状态码({status}): {body}"));
        }
        let jm_resp = serde_json::from_str::<JmResp>(&body)
            .wrap_err(format!("将body解析为JmResp失败: {body}"))?;
        if jm_resp.code != 200 {
            return Err(eyre!("移动收藏夹失败，预料之外的code: {jm_resp:?}"));
        }
        let data = jm_resp
            .data
            .as_str()
            .ok_or_eyre(format!("移动收藏夹失败，data字段不是字符串: {jm_resp:?}"))?;
        let data = decrypt_data(ts, data)?;
        // 这个接口即使操作没生效也可能返回 code 200，要自己看 status
        let action_resp_data: FavoriteFolderActionRespData =
            serde_json::from_str(&data).unwrap_or_default();
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
        let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let http_resp = self.jm_get(ApiPath::GetCategories, None, ts).await?;

        let status = http_resp.status();
        let body = http_resp.text().await?;
        if status != reqwest::StatusCode::OK {
            return Err(eyre!("获取分类失败，预料之外的状态码({status}): {body}"));
        }

        let jm_resp = serde_json::from_str::<JmResp>(&body)
            .wrap_err(format!("将body解析为JmResp失败: {body}"))?;
        if jm_resp.code != 200 {
            return Err(eyre!("获取分类失败，预料之外的code: {jm_resp:?}"));
        }

        let data = jm_resp
            .data
            .as_str()
            .ok_or_eyre(format!("获取分类失败，data字段不是字符串: {jm_resp:?}"))?;
        let data = decrypt_data(ts, data)?;

        let value: serde_json::Value = serde_json::from_str(&data)
            .wrap_err(format!("将解密后的分类数据解析为JSON失败: {data}"))?;

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
        let mut errors = Vec::with_capacity(candidates.len());
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
                    let message = err.to_string();
                    tracing::warn!(domain = *domain, message, "图片线路失败，自动换下一条");
                    lines::mark_image_line_failed(domain);
                    let brief = message.lines().next().unwrap_or("未知错误").to_string();
                    errors.push(format!("{domain}: {brief}"));
                }
            }
        }

        Err(eyre!(
            "图片线路全部失败（已尝试{}条）: {}",
            candidates.len(),
            errors.join("; ")
        ))
    }

    #[instrument(level = "error", skip_all, fields(url = url))]
    async fn fetch_img_data_and_format_once(
        &self,
        url: &str,
    ) -> eyre::Result<(Bytes, ImageFormat)> {
        let request = self.img_client.read().get(url).header("user-agent", USER_AGENT);
        let http_resp = request.send().await?;

        let status = http_resp.status();
        if status != StatusCode::OK {
            let text = http_resp.text().await?;
            let err = eyre!("下载图片失败，预料之外的状态码: {text}");
            return Err(err);
        }

        let mut image_data = http_resp.bytes().await?;

        if image_data.is_empty() {
            // 如果图片为空，说明jm那边缓存失效了，带上时间戳再次请求，以避免缓存
            let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
            let query = json!({"ts": ts});
            let request = self.img_client.read().get(url).query(&query);

            let http_resp = request.send().await?;
            let status = http_resp.status();
            if status != StatusCode::OK {
                let text = http_resp.text().await?;
                let err = eyre!("下载图片失败，预料之外的状态码: {text}");
                return Err(err);
            }

            image_data = http_resp.bytes().await?;
        }

        let format = image::guess_format(&image_data)
            .wrap_err("无法从图片数据中猜测出图片格式，可能图片数据不完整或已损坏")?;

        Ok((image_data, format))
    }
}
