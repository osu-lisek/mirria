use std::{fmt, sync::Arc};



use crate::{
    crawler::Context,
    osu::types::{Beatmap, Beatmapset},
};

#[derive(Debug)]
pub enum DatabaseError {
    RecordNotFound,
    InternalDatabaseError,
}

impl std::error::Error for DatabaseError {}

impl fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DatabaseError::RecordNotFound => write!(f, "Record not found."),
            DatabaseError::InternalDatabaseError => write!(f, "Internal database error."),
        }
    }
}

pub async fn get_beatmap_by_id(ctx: Context, id: i64) -> Result<Beatmap, DatabaseError> {
    let response = ctx
        .meili_client
        .index("beatmapset")
        .search()
        .with_filter(format!("beatmaps.id = {}", id).as_str())
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

    let beatmap = beatmapset.beatmaps.iter().find(|x| x.id == id).unwrap();

    Ok(beatmap.clone())
}
