use jules_rs::{Activity, JulesClient, ListActivitiesParams, JulesError};

pub async fn fetch_all_activities(client: &JulesClient, session_name: &str) -> Result<Vec<Activity>, JulesError> {
    let mut activities = Vec::new();
    let mut page_token = None;
    loop {
        let response = client
            .list_activities(
                session_name,
                ListActivitiesParams {
                    page_size: Some(100),
                    page_token: page_token.clone(),
                    ..Default::default()
                },
            )
            .await?;
        activities.extend(response.activities);
        match response.next_page_token {
            Some(token) if !token.is_empty() => page_token = Some(token),
            _ => break,
        }
    }
    Ok(activities)
}
