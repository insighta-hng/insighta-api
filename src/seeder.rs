use std::path::Path;

use crate::{
    models::seed::SeedFile,
    repo::profile::{Profile, ProfileRepo},
};
use chrono::Utc;
use uuid::Uuid;

pub async fn run(repo: ProfileRepo) {
    let seed_path = Path::new("seed_profiles.json");
    if !seed_path.exists() {
        tracing::warn!("seed_profiles.json not found, skipping seed");
        return;
    }

    let raw_seed_data = match std::fs::read_to_string(seed_path) {
        Ok(data) => data,
        Err(e) => {
            tracing::error!("Failed to read seed file: {}", e);
            return;
        }
    };

    let seed_file: SeedFile = match serde_json::from_str(&raw_seed_data) {
        Ok(f) => f,
        Err(e) => {
            tracing::error!("Failed to parse seed file: {}", e);
            return;
        }
    };

    let total = seed_file.profiles.len();
    let now = Utc::now();

    let profiles: Vec<Profile> = seed_file
        .profiles
        .into_iter()
        .map(|sp| Profile {
            id: Uuid::now_v7(),
            name: sp.name,
            gender: sp.gender,
            gender_probability: sp.gender_probability,
            age: sp.age,
            age_group: sp.age_group,
            country_id: sp.country_id,
            country_name: sp.country_name,
            country_probability: sp.country_probability,
            created_at: now,
        })
        .collect();

    match repo.insert_many_profiles(profiles).await {
        Ok(inserted) => {
            let skipped = total as u64 - inserted;
            tracing::info!(
                total = total,
                inserted = inserted,
                skipped = skipped,
                "Database seeding complete"
            );
        }
        Err(e) => {
            tracing::error!("Database seeding failed: {}", e);
        }
    }
}
