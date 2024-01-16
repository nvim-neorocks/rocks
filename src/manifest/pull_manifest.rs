use anyhow::Result;

pub async fn manifest_from_server(url: String, lua_version: Option<&String>) -> Result<String> {
    Ok(
        reqwest::get(url + &lua_version.map(|s| format!("-{}", s)).unwrap_or_default())
            .await?
            .text()
            .await?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    pub async fn get_manifest() {
        manifest_from_server("https://luarocks.org/manifest".into(), None)
            .await
            .unwrap();
    }

    #[tokio::test]
    pub async fn get_manifest_for_5_1() {
        manifest_from_server("https://luarocks.org/manifest".into(), Some(&"5.1".into()))
            .await
            .unwrap();
    }
}
