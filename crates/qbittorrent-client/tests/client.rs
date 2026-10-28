//! Tests against a mock qBittorrent server.

use qbittorrent_client::{Client, ClientConfig, Credentials, Error, Version};
use wiremock::matchers::{body_string, header, header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn password() -> Credentials {
    Credentials::Password {
        username: "admin".into(),
        password: "secret pass&".into(),
    }
}

fn client(server: &MockServer, prefix: &str, credentials: Credentials) -> Client {
    let config = ClientConfig::new(&format!("{}{prefix}", server.uri()), credentials).unwrap();
    Client::new(config).unwrap()
}

async fn mount_versions(server: &MockServer, prefix: &str, version: &str) {
    Mock::given(method("GET"))
        .and(path(format!("{prefix}/api/v2/app/version")))
        .respond_with(ResponseTemplate::new(200).set_body_string(version))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{prefix}/api/v2/app/webapiVersion")))
        .respond_with(ResponseTemplate::new(200).set_body_string("2.11.4"))
        .mount(server)
        .await;
}

fn login_ok(sid: &str) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header(
            "set-cookie",
            format!("SID={sid}; HttpOnly; path=/").as_str(),
        )
        .set_body_string("Ok.")
}

#[tokio::test]
async fn connects_with_password_and_sends_cookie_and_referer() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v2/auth/login"))
        .and(header("referer", format!("{}/", server.uri()).as_str()))
        .and(body_string("username=admin&password=secret+pass%26"))
        .respond_with(login_ok("abc"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v2/app/version"))
        .and(header("cookie", "SID=abc"))
        .respond_with(ResponseTemplate::new(200).set_body_string("v5.1.2"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v2/app/webapiVersion"))
        .and(header("cookie", "SID=abc"))
        .respond_with(ResponseTemplate::new(200).set_body_string("2.11.4"))
        .mount(&server)
        .await;

    let info = client(&server, "", password()).connect().await.unwrap();
    assert_eq!(info.version, Version::new(5, 1, 2));
    assert_eq!(info.api_version, Version::new(2, 11, 4));
}

#[tokio::test]
async fn accepts_empty_login_response_of_newer_servers() {
    // qBittorrent 5.3 answers 204 and names the cookie after the port
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v2/auth/login"))
        .respond_with(ResponseTemplate::new(204).insert_header(
            "set-cookie",
            "QBT_SID_8080=xyz; HttpOnly; SameSite=Lax; path=/",
        ))
        .mount(&server)
        .await;
    Mock::given(path("/api/v2/app/version"))
        .and(header("cookie", "QBT_SID_8080=xyz"))
        .respond_with(ResponseTemplate::new(200).set_body_string("v5.3.0"))
        .mount(&server)
        .await;
    Mock::given(path("/api/v2/app/webapiVersion"))
        .and(header("cookie", "QBT_SID_8080=xyz"))
        .respond_with(ResponseTemplate::new(200).set_body_string("2.17.0"))
        .mount(&server)
        .await;

    let info = client(&server, "", password()).connect().await.unwrap();
    assert_eq!(info.version, Version::new(5, 3, 0));
}

#[tokio::test]
async fn login_page_from_another_app_is_not_qbittorrent() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v2/auth/login"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>Sign in</html>"))
        .mount(&server)
        .await;

    let err = client(&server, "", password()).connect().await.unwrap_err();
    assert!(matches!(err, Error::NotQbittorrent), "{err:?}");
}

#[tokio::test]
async fn supports_path_prefix() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/qbt/api/v2/auth/login"))
        .respond_with(login_ok("abc"))
        .mount(&server)
        .await;
    mount_versions(&server, "/qbt", "v5.0.0").await;

    let info = client(&server, "/qbt", password()).connect().await.unwrap();
    assert_eq!(info.version, Version::new(5, 0, 0));
}

#[tokio::test]
async fn api_key_uses_bearer_header_without_login() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v2/auth/login"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(header(
        "authorization",
        "Bearer qbt_0123456789abcdefghijklmnopqr",
    ))
    .and(path("/api/v2/app/version"))
    .respond_with(ResponseTemplate::new(200).set_body_string("v5.2.0"))
    .mount(&server)
    .await;
    Mock::given(header(
        "authorization",
        "Bearer qbt_0123456789abcdefghijklmnopqr",
    ))
    .and(path("/api/v2/app/webapiVersion"))
    .respond_with(ResponseTemplate::new(200).set_body_string("2.14.1"))
    .mount(&server)
    .await;

    let credentials = Credentials::ApiKey("qbt_0123456789abcdefghijklmnopqr".into());
    let info = client(&server, "", credentials).connect().await.unwrap();
    assert_eq!(info.api_version, Version::new(2, 14, 1));
}

#[tokio::test]
async fn invalid_api_key_is_unauthorized() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v2/app/version"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let err = client(&server, "", Credentials::ApiKey("qbt_wrong".into()))
        .connect()
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Unauthorized), "{err:?}");
}

#[tokio::test]
async fn invalid_credentials_old_style() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v2/auth/login"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Fails."))
        .mount(&server)
        .await;

    let err = client(&server, "", password()).connect().await.unwrap_err();
    assert!(matches!(err, Error::InvalidCredentials), "{err:?}");
}

#[tokio::test]
async fn invalid_credentials_new_style() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v2/auth/login"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Fails."))
        .mount(&server)
        .await;

    let err = client(&server, "", password()).connect().await.unwrap_err();
    assert!(matches!(err, Error::InvalidCredentials), "{err:?}");
}

#[tokio::test]
async fn banned_ip() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v2/auth/login"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;

    let err = client(&server, "", password()).connect().await.unwrap_err();
    assert!(matches!(err, Error::Banned), "{err:?}");
}

#[tokio::test]
async fn rejects_old_servers() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v2/auth/login"))
        .respond_with(login_ok("abc"))
        .mount(&server)
        .await;
    mount_versions(&server, "", "v4.6.7").await;

    let err = client(&server, "", password()).connect().await.unwrap_err();
    assert!(
        matches!(err, Error::UnsupportedVersion { found, .. } if found == Version::new(4, 6, 7)),
        "{err:?}"
    );
}

#[tokio::test]
async fn detects_non_qbittorrent_servers() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v2/app/version"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<!doctype html><html></html>"))
        .mount(&server)
        .await;
    let err = client(&server, "", Credentials::None)
        .connect()
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotQbittorrent), "{err:?}");

    let empty = MockServer::start().await;
    let err = client(&empty, "", Credentials::None)
        .connect()
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotQbittorrent), "{err:?}");
}

#[tokio::test]
async fn logs_in_again_when_session_expires() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v2/auth/login"))
        .respond_with(login_ok("first"))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v2/auth/login"))
        .respond_with(login_ok("second"))
        .expect(1)
        .mount(&server)
        .await;
    // The first session is "expired": requests with it get 403
    Mock::given(header("cookie", "SID=first"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    mount_versions(&server, "", "v5.1.0").await;

    let info = client(&server, "", password()).connect().await.unwrap();
    assert_eq!(info.version, Version::new(5, 1, 0));
}

#[tokio::test]
async fn network_errors_are_transient() {
    // Nothing listens on this port
    let config = ClientConfig::new("http://127.0.0.1:9", Credentials::None).unwrap();
    let err = Client::new(config).unwrap().connect().await.unwrap_err();
    assert!(matches!(err, Error::Network(_)), "{err:?}");
    assert!(err.is_transient());
}

#[tokio::test]
async fn no_auth_header_with_password_credentials() {
    let server = MockServer::start().await;
    Mock::given(header_exists("authorization"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(path("/api/v2/auth/login"))
        .respond_with(login_ok("abc"))
        .mount(&server)
        .await;
    mount_versions(&server, "", "v5.0.1").await;

    client(&server, "", password()).connect().await.unwrap();
}

#[tokio::test]
async fn fetches_maindata_with_rid() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v2/sync/maindata"))
        .and(wiremock::matchers::query_param("rid", "7"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"rid":8,"torrents":{"abc":{"dlspeed":1024}}}"#),
        )
        .mount(&server)
        .await;

    let data = client(&server, "", Credentials::None)
        .sync_maindata(7)
        .await
        .unwrap();
    assert_eq!(data.rid, 8);
    assert_eq!(data.torrents["abc"].dlspeed, Some(1024));
}

#[tokio::test]
async fn actions_post_the_expected_forms() {
    let server = MockServer::start().await;
    let expectations = [
        ("/api/v2/torrents/start", "hashes=a%7Cb"),
        ("/api/v2/torrents/stop", "hashes=a%7Cb"),
        ("/api/v2/torrents/delete", "hashes=a%7Cb&deleteFiles=true"),
        (
            "/api/v2/torrents/setCategory",
            "hashes=a%7Cb&category=linux+isos",
        ),
        (
            "/api/v2/torrents/createCategory",
            "category=linux&savePath=%2Fdata",
        ),
        (
            "/api/v2/torrents/addTags",
            "hashes=a%7Cb&tags=iso%2Cweekend",
        ),
        ("/api/v2/torrents/removeTags", "hashes=a%7Cb&tags=iso"),
        (
            "/api/v2/torrents/setLocation",
            "hashes=a%7Cb&location=%2Fdata%2Fmovies",
        ),
        ("/api/v2/torrents/rename", "hash=a&name=New+name"),
        ("/api/v2/transfer/toggleSpeedLimitsMode", ""),
    ];
    for (endpoint, body) in expectations {
        Mock::given(method("POST"))
            .and(path(endpoint))
            .and(body_string(body))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
    }

    let client = client(&server, "", Credentials::None);
    let hashes = ["a".to_owned(), "b".to_owned()];
    client.start(&hashes).await.unwrap();
    client.stop(&hashes).await.unwrap();
    client.delete(&hashes, true).await.unwrap();
    client.set_category(&hashes, "linux isos").await.unwrap();
    client.create_category("linux", "/data").await.unwrap();
    client
        .add_tags(&hashes, &["iso".into(), "weekend".into()])
        .await
        .unwrap();
    client.remove_tags(&hashes, &["iso".into()]).await.unwrap();
    client.set_location(&hashes, "/data/movies").await.unwrap();
    client.rename("a", "New name").await.unwrap();
    client.toggle_alternative_speed_limits().await.unwrap();
}

#[tokio::test]
async fn action_errors_are_reported() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v2/torrents/setLocation"))
        .respond_with(ResponseTemplate::new(409).set_body_string("Cannot create directory"))
        .mount(&server)
        .await;

    let err = client(&server, "", Credentials::None)
        .set_location(&["a".into()], "/nope")
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::Http { status, body } if status.as_u16() == 409 && body == "Cannot create directory"),
        "{err:?}"
    );
}

mod add {
    use qbittorrent_client::{AddTorrents, ContentLayout, Credentials, Error, TorrentFileUpload};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    use super::client;

    fn body(request: &Request) -> String {
        String::from_utf8_lossy(&request.body).into_owned()
    }

    #[tokio::test]
    async fn sends_urls_files_and_options_as_multipart() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v2/torrents/add"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"added_torrent_ids":["abc"],"failure_count":0,"pending_count":0,"success_count":1}"#,
            ))
            .expect(1)
            .mount(&server)
            .await;

        let torrents = AddTorrents {
            urls: vec![
                "magnet:?xt=urn:btih:abc".into(),
                "https://example.com/a.torrent".into(),
            ],
            files: vec![TorrentFileUpload {
                file_name: "debian.torrent".into(),
                data: b"d4:infodee".to_vec(),
            }],
            save_path: Some("/data/isos".into()),
            category: Some("linux".into()),
            tags: vec!["iso".into(), "weekend".into()],
            stopped: true,
            content_layout: Some(ContentLayout::Subfolder),
        };
        client(&server, "", Credentials::None)
            .add(&torrents)
            .await
            .unwrap();

        let requests = server.received_requests().await.unwrap();
        let body = body(&requests[0]);
        for expected in [
            "name=\"urls\"\r\n\r\nmagnet:?xt=urn:btih:abc\nhttps://example.com/a.torrent\r\n",
            "name=\"torrents\"; filename=\"debian.torrent\"\r\nContent-Type: application/x-bittorrent\r\n\r\nd4:infodee\r\n",
            "name=\"savepath\"\r\n\r\n/data/isos\r\n",
            "name=\"category\"\r\n\r\nlinux\r\n",
            "name=\"tags\"\r\n\r\niso,weekend\r\n",
            "name=\"stopped\"\r\n\r\ntrue\r\n",
            "name=\"contentLayout\"\r\n\r\nSubfolder\r\n",
        ] {
            assert!(body.contains(expected), "missing {expected:?} in {body}");
        }
    }

    #[tokio::test]
    async fn omits_default_options() {
        let server = MockServer::start().await;
        Mock::given(path("/api/v2/torrents/add"))
            .respond_with(ResponseTemplate::new(200).set_body_string("Ok."))
            .mount(&server)
            .await;

        let torrents = AddTorrents {
            urls: vec!["magnet:?xt=urn:btih:abc".into()],
            ..Default::default()
        };
        client(&server, "", Credentials::None)
            .add(&torrents)
            .await
            .unwrap();

        let body = body(&server.received_requests().await.unwrap()[0]);
        for absent in ["savepath", "category", "tags", "contentLayout", "torrents"] {
            assert!(
                !body.contains(&format!("name=\"{absent}\"")),
                "unexpected {absent}"
            );
        }
        assert!(body.contains("name=\"stopped\"\r\n\r\nfalse"));
    }

    #[tokio::test]
    async fn pending_magnets_are_a_success() {
        let server = MockServer::start().await;
        Mock::given(path("/api/v2/torrents/add"))
            .respond_with(ResponseTemplate::new(202).set_body_string(
                r#"{"added_torrent_ids":[],"failure_count":0,"pending_count":1,"success_count":0}"#,
            ))
            .mount(&server)
            .await;

        let torrents = AddTorrents {
            urls: vec!["https://example.com/a.torrent".into()],
            ..Default::default()
        };
        client(&server, "", Credentials::None)
            .add(&torrents)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn rejections_in_every_server_version() {
        for response in [
            ResponseTemplate::new(200).set_body_string("Fails."),
            ResponseTemplate::new(409),
            ResponseTemplate::new(415).set_body_string("Error: 'x' is not a valid torrent file."),
        ] {
            let server = MockServer::start().await;
            Mock::given(path("/api/v2/torrents/add"))
                .respond_with(response)
                .mount(&server)
                .await;
            let torrents = AddTorrents {
                urls: vec!["magnet:?xt=urn:btih:abc".into()],
                ..Default::default()
            };
            let err = client(&server, "", Credentials::None)
                .add(&torrents)
                .await
                .unwrap_err();
            assert!(matches!(err, Error::TorrentRejected), "{err:?}");
        }
    }
}
