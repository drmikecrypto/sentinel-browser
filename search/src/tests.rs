use tokio::sync::mpsc;
use crate::{SearchResult, BlockchainIndexer, Command, NetworkType, Indexer, ResultBadge};

#[tokio::test]
async fn test_search_result_serialization() {
    let result = SearchResult {
        title: "Test".to_string(),
        url: "http://test.onion".to_string(),
        description: "Test description".to_string(),
        source: NetworkType::Tor,
        verified: false,
        badge: ResultBadge::Onion,
    };
    let serialized = serde_json::to_string(&result).unwrap();
    let deserialized: SearchResult = serde_json::from_str(&serialized).unwrap();
    assert_eq!(result.url, deserialized.url);
}

#[tokio::test]
async fn test_dht_command_routing() {
    let (tx, mut rx) = mpsc::channel(1);
    let indexer = BlockchainIndexer::new(tx);

    tokio::spawn(async move {
        if let Some(Command::Get { key: _, sender }) = rx.recv().await {
            let results = vec![SearchResult {
                title: "DHT Result".to_string(),
                url: "ipfs://123".to_string(),
                description: "Found".to_string(),
                source: NetworkType::Blockchain("IPFS".to_string()),
                verified: true,
                badge: ResultBadge::Ipfs,
            }];
            let val = serde_json::to_vec(&results).unwrap();
            let _ = sender.send(Ok(val));
        }
    });

    let results = indexer.search("test").await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "DHT Result");
}

#[tokio::test]
async fn test_local_index_seeds() {
    let dir = std::env::temp_dir().join(format!("sentinel-test-index-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let idx = crate::TantivyIndex::open_or_create(&dir).unwrap();
    idx.bulk_index(&crate::seeds::seed_documents()).unwrap();
    let results = idx.search("tor", 5).unwrap();
    assert!(!results.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
