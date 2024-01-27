use tracing::error;

use crate::{crawler::Context, osu::types::Beatmapset};

use super::beatmaps::DatabaseError;


pub async fn get_beatmapset_by_hash(ctx: Context, checksum: impl ToString) -> Result<Beatmapset, DatabaseError> {
    let response = ctx
        .meili_client
        .index("beatmapset")
        .search()
        .with_filter(format!("beatmaps.checksum = {}", checksum.to_string()).as_str())
        .execute::<Beatmapset>()
        .await;

    if response.is_err() {
        return Err(DatabaseError::InternalDatabaseError);
    }

    let response = response.unwrap();

    if response.hits.len() == 0 {
        return Err(DatabaseError::RecordNotFound);
    }

    let beatmapset = &response.hits.first().unwrap().result;

    Ok(beatmapset.clone())
}


pub async fn get_beatmapset_by_id(ctx: Context, id: i64) -> Result<Beatmapset, DatabaseError> {
    let response = ctx
        .meili_client
        .index("beatmapset")
        .search()
        .with_filter(format!("id = {}", id).as_str())
        .execute::<Beatmapset>()
        .await;

    if response.is_err() {
        return Err(DatabaseError::InternalDatabaseError);
    }

    let response = response.unwrap();

    if response.hits.len() == 0 {
        return Err(DatabaseError::RecordNotFound);
    }

    let beatmapset = &response.hits.first().unwrap().result;

    Ok(beatmapset.clone())
}


pub async fn get_beatmapset_by_beatmap_id(ctx: Context, id: i64) -> Result<Beatmapset, DatabaseError> {
    let response = ctx
        .meili_client
        .index("beatmapset")
        .search()
        .with_filter(format!("beatmaps.id = {}", id).as_str())
        .execute::<Beatmapset>()
        .await;

    if response.is_err() {
    
        let err = response.unwrap_err();
        error!("{:#?}", err);
        return Err(DatabaseError::InternalDatabaseError);
    }

    let response = response.unwrap();

    if response.hits.len() == 0 {
        return Err(DatabaseError::RecordNotFound);
    }

    let beatmapset = &response.hits.first().unwrap().result;

    Ok(beatmapset.clone())
}
