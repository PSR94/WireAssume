use serde_json::json;
use wireassume_model::{
    Body, CorpusStore, Header, Interaction, InteractionMetadata, RedactionPolicy, RequestRecord,
    ResponseRecord, REDACTED,
};

fn sample() -> Interaction {
    Interaction {
        request: RequestRecord {
            method: "GET".into(),
            uri: "https://people.test/customers/123?api_key=secret".into(),
            headers: vec![Header {
                name: "Authorization".into(),
                value: "Bearer top-secret".into(),
            }],
            body: Body::Empty,
        },
        response: ResponseRecord {
            status: 200,
            headers: vec![Header {
                name: "Content-Type".into(),
                value: "application/json".into(),
            }],
            body: Body::Json(json!({"id": "123", "email": "alice@example.com"})),
        },
        metadata: InteractionMetadata::fixture("peoplecrm", "customer-profile"),
    }
}

#[test]
fn corpus_ids_are_deterministic_and_secrets_are_not_persisted() {
    let temp = tempfile::tempdir().unwrap();
    let store = CorpusStore::new(temp.path());
    let first = store
        .persist(&sample(), &RedactionPolicy::default())
        .unwrap();
    let second = store
        .persist(&sample(), &RedactionPolicy::default())
        .unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(first.content_sha256, second.content_sha256);

    let request = std::fs::read_to_string(first.directory.join("request.json")).unwrap();
    assert!(!request.contains("top-secret"));
    assert!(!request.contains("secret"));
    assert!(request.contains(REDACTED));
}
