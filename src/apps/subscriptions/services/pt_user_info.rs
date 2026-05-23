use tokimo_pt_search::pt_user_info::{PtSiteInput, PtUserInfo, fetch_user_info as pt_fetch};

use crate::apps::subscriptions::models::pt_site::{PtSiteDto, PtUserInfoDto};

pub async fn fetch_user_info(site: &PtSiteDto) -> Result<PtUserInfoDto, String> {
    let input = PtSiteInput {
        site_id: site.site_id.clone(),
        domain: site.domain.clone(),
        auth_type: site.auth_type.clone(),
        cookies: site.cookies.clone(),
        api_key: site.api_key.clone(),
    };
    let info = pt_fetch(&input).await?;
    Ok(to_dto(info))
}

fn to_dto(info: PtUserInfo) -> PtUserInfoDto {
    PtUserInfoDto {
        uid: info.uid,
        username: info.username,
        uploaded: info.uploaded,
        downloaded: info.downloaded,
        share_ratio: info.share_ratio,
        seeding: info.seeding,
        leeching: info.leeching,
        vip_group: info.vip_group,
        bonus: info.bonus,
    }
}
