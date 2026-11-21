//! 本机 Chrome / Edge 书签文件夹只读扫描：
//! - 仅读取标准 `Bookmarks` 明文 JSON 文件（不碰密码、Cookie、历史记录或 SQLite 会话库）；
//! - 供用户在工作空间的「文件与网址」中一键选择书签夹批量导入常用网页。

use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkUrl {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkFolder {
    pub browser: String,
    pub folder_path: String,
    pub urls: Vec<BookmarkUrl>,
}

/// 扫描本机 Chrome 与 Edge 默认配置下的非空书签文件夹
pub fn list_folders() -> Vec<BookmarkFolder> {
    let Some(local_app_data) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) else {
        return Vec::new();
    };

    let sources = [
        (
            "Chrome",
            local_app_data.join(r"Google\Chrome\User Data\Default\Bookmarks"),
        ),
        (
            "Edge",
            local_app_data.join(r"Microsoft\Edge\User Data\Default\Bookmarks"),
        ),
    ];

    let mut out = Vec::new();
    for (browser, path) in sources {
        if let Ok(text) = std::fs::read_to_string(&path) {
            out.extend(parse_bookmarks_json(browser, &text));
        }
    }
    out
}

pub fn parse_bookmarks_json(browser: &str, text: &str) -> Vec<BookmarkFolder> {
    let Ok(val) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let Some(roots) = val.get("roots").and_then(|v| v.as_object()) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for (root_key, node) in roots {
        let root_label = match root_key.as_str() {
            "bookmark_bar" => "书签栏",
            "other" => "其他书签",
            "synced" => "移动设备书签",
            other => other,
        };
        collect_folder(browser, root_label, node, &mut out);
    }
    out
}

fn collect_folder(browser: &str, prefix: &str, node: &Value, out: &mut Vec<BookmarkFolder>) {
    let Some(children) = node.get("children").and_then(|v| v.as_array()) else {
        return;
    };

    let mut direct_urls = Vec::new();
    for child in children {
        let kind = child.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if kind == "url" {
            let url = child.get("url").and_then(|v| v.as_str()).unwrap_or("").trim();
            if url.starts_with("http://") || url.starts_with("https://") {
                let name = child
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(url)
                    .trim();
                direct_urls.push(BookmarkUrl {
                    name: if name.is_empty() { url.to_string() } else { name.to_string() },
                    url: url.to_string(),
                });
            }
        } else if kind == "folder" {
            let name = child
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("未命名")
                .trim();
            let sub = if name.is_empty() {
                prefix.to_string()
            } else {
                format!("{prefix} / {name}")
            };
            collect_folder(browser, &sub, child, out);
        }
    }

    if !direct_urls.is_empty() {
        out.push(BookmarkFolder {
            browser: browser.to_string(),
            folder_path: prefix.to_string(),
            urls: direct_urls,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chromium_bookmarks_folders_and_filters_non_http() {
        let sample = r#"{
          "roots": {
            "bookmark_bar": {
              "type": "folder",
              "children": [
                { "type": "url", "name": "GitHub", "url": "https://github.com" },
                { "type": "url", "name": "LocalJS", "url": "javascript:alert(1)" },
                {
                  "type": "folder",
                  "name": "AI 工具",
                  "children": [
                    { "type": "url", "name": "ChatGPT", "url": "https://chatgpt.com" },
                    { "type": "url", "name": "Docs", "url": "http://localhost:3000" }
                  ]
                }
              ]
            }
          }
        }"#;

        let folders = parse_bookmarks_json("Chrome", sample);
        assert_eq!(folders.len(), 2);
        let ai = folders.iter().find(|f| f.folder_path == "书签栏 / AI 工具").unwrap();
        assert_eq!(ai.urls.len(), 2);
        assert_eq!(ai.urls[0].url, "https://chatgpt.com");

        let bar = folders.iter().find(|f| f.folder_path == "书签栏").unwrap();
        assert_eq!(bar.urls.len(), 1);
        assert_eq!(bar.urls[0].name, "GitHub");
    }
}
