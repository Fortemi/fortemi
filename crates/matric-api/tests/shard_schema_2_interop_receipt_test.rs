use sha2::Digest;

#[test]
fn shard_schema_2_full_v1_interop_receipt_binds_the_paired_deliveries() {
    let receipt: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/shards/external/schema-2-full-v1.interop-receipt.json"
    ))
    .unwrap();
    let runtime_receipt_bytes = include_bytes!(
        "../../../tests/fixtures/shards/external/schema-2-runtime.implementation-receipt.json"
    );
    let react_archive =
        include_bytes!("../../../tests/fixtures/shards/external/react-2026.7.13/react-full-v1.shard");
    let aiwg_archive =
        include_bytes!("../../../tests/fixtures/shards/external/aiwg-2026.7.13/aiwg-full-v1.shard");

    assert_eq!(
        receipt["schemaVersion"],
        "fortemi.knowledge-shard.full-v1-interop-receipt.v1"
    );
    assert_eq!(
        receipt["status"],
        "delivered-cross-repository-conformance-passed"
    );
    assert_eq!(receipt["tuple"]["schemaVersion"], "2.0.0");
    assert_eq!(receipt["tuple"]["profile"], "full-v1");
    assert_eq!(
        receipt["authority"]["commit"],
        "6343bd899958445bbc7e7e87b0dc92a8429d5a06"
    );
    assert_eq!(
        receipt["react"]["advertisementCommit"],
        "1c0d5dba05c3e675c01cf53e93b6d082f2174b54"
    );
    assert_eq!(
        receipt["react"]["crossRepositoryReceipt"]["sha256"],
        "f9be3191bf0d7ce232cc2ea54e181d783e775111affb4f85f64873113b1ded5e"
    );
    assert_eq!(
        receipt["fortemi"]["runtimeReceipt"]["sha256"],
        hex::encode(sha2::Sha256::digest(runtime_receipt_bytes))
    );
    assert_eq!(
        receipt["archives"]["react"]["sha256"],
        hex::encode(sha2::Sha256::digest(react_archive))
    );
    assert_eq!(
        receipt["archives"]["aiwg"]["sha256"],
        hex::encode(sha2::Sha256::digest(aiwg_archive))
    );
    assert_eq!(receipt["react"]["ci"]["conclusion"], "success");
    assert_eq!(receipt["fortemi"]["ci"]["conclusion"], "success");

    let expected_cells = [
        "pglite-full-v1-to-pglite",
        "aiwg-full-v1-to-pglite",
        "pglite-full-v1-to-fortemi",
        "aiwg-full-v1-to-fortemi",
    ];
    let cells = receipt["cells"].as_array().unwrap();
    assert_eq!(cells.len(), expected_cells.len());
    for (cell, expected) in cells.iter().zip(expected_cells) {
        assert_eq!(cell["id"], expected);
        assert_eq!(cell["status"], "passed");
    }
    assert_eq!(receipt["coverage"].as_array().unwrap().len(), 22);
    assert_eq!(receipt["claims"]["fullV1Interoperability"], true);
    assert_eq!(receipt["claims"]["suiteWide"], false);
    assert_eq!(receipt["claims"]["completeBackup"], false);
}
