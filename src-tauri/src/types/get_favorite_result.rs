use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use eyre::WrapErr;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tracing::instrument;

use crate::{
    local_index,
    responses::{
        CategoryRespData, CategorySubRespData, ComicInFavoriteRespData, FavoriteFolderRespData,
        GetFavoriteRespData,
    },
    utils,
};

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GetFavoriteResult {
    pub list: Vec<ComicInFavorite>,
    pub folder_list: Vec<FavoriteFolderRespData>,
    pub total: i64,
    pub count: i64,
}

impl GetFavoriteResult {
    #[instrument(level = "error", skip_all)]
    pub fn from_resp_data(
        app: &AppHandle,
        resp_data: GetFavoriteRespData,
    ) -> eyre::Result<GetFavoriteResult> {
        let id_to_dir_map = utils::create_id_to_dir_map(app)?;
        let exported_ids = local_index::exported_id_set(app);

        let list = resp_data
            .list
            .into_iter()
            .map(|comic| ComicInFavorite::from_resp_data(comic, &id_to_dir_map, &exported_ids))
            .collect::<eyre::Result<_>>()?;

        let total: i64 = resp_data.total.parse().wrap_err("将total解析为i64失败")?;

        let get_favorite_result = GetFavoriteResult {
            list,
            folder_list: resp_data.folder_list,
            total,
            count: resp_data.count,
        };

        Ok(get_favorite_result)
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ComicInFavorite {
    pub id: i64,
    pub author: String,
    pub description: Option<String>,
    pub name: String,
    pub latest_ep: Option<String>,
    pub latest_ep_aid: Option<String>,
    pub image: String,
    pub category: CategoryRespData,
    pub category_sub: CategorySubRespData,
    pub is_downloaded: bool,
    /// 导出目录里有没有这本（"列出全部漫画"里标记已导出）
    pub is_exported: bool,
    pub comic_download_dir: PathBuf,
}

impl ComicInFavorite {
    pub fn from_resp_data(
        resp_data: ComicInFavoriteRespData,
        id_to_dir_map: &HashMap<i64, PathBuf>,
        exported_ids: &HashSet<i64>,
    ) -> eyre::Result<ComicInFavorite> {
        let id: i64 = resp_data.id.parse().wrap_err("将id解析为i64失败")?;

        let mut comic = ComicInFavorite {
            id,
            author: resp_data.author,
            description: resp_data.description,
            name: resp_data.name,
            latest_ep: resp_data.latest_ep,
            latest_ep_aid: resp_data.latest_ep_aid,
            image: resp_data.image,
            category: resp_data.category,
            category_sub: resp_data.category_sub,
            is_downloaded: false,
            is_exported: false,
            comic_download_dir: PathBuf::new(),
        };

        comic.update_fields(id_to_dir_map, exported_ids);

        Ok(comic)
    }

    pub fn update_fields(
        &mut self,
        id_to_dir_map: &HashMap<i64, PathBuf>,
        exported_ids: &HashSet<i64>,
    ) {
        if let Some(comic_download_dir) = id_to_dir_map.get(&self.id) {
            self.comic_download_dir = comic_download_dir.clone();
            self.is_downloaded = true;
        }
        self.is_exported = exported_ids.contains(&self.id);
    }
}
