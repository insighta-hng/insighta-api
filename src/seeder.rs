use std::path::Path;

use crate::{
    models::{profile::Profile, seed::SeedFile},
    repo::profile::ProfileRepo,
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
        Ok(parsed) => parsed,
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
        .map(|seed_profile| Profile {
            id: Uuid::now_v7(),
            name: seed_profile.name,
            gender: seed_profile.gender,
            gender_probability: seed_profile.gender_probability,
            age: seed_profile.age,
            age_group: seed_profile.age_group,
            country_id: seed_profile.country_id,
            country_name: seed_profile.country_name,
            country_probability: seed_profile.country_probability,
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
