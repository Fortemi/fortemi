use sha2::Digest;

#[test]
fn shard_schema_2_runtime_receipt_binds_delivered_main_and_external_producers() {
    let receipt: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/shards/external/schema-2-runtime.implementation-receipt.json"
    ))
    .unwrap();
    assert_eq!(
        receipt["schemaVersion"],
        "fortemi.knowledge-shard.schema-2-runtime-receipt.v1"
    );
    assert_eq!(receipt["status"], "delivered-main-conformance-passed");
    assert_eq!(
        receipt["authority"]["commit"],
        "6343bd899958445bbc7e7e87b0dc92a8429d5a06"
    );
    assert_eq!(receipt["authority"]["contractRevision"], "20");
    assert_eq!(
        receipt["authority"]["contractSha256"],
        hex::encode(sha2::Sha256::digest(include_bytes!(
            "../../../contracts/knowledge-shard/2.0.0/contract.json"
        )))
    );
    assert_eq!(
        receipt["authority"]["fieldSemanticsSha256"],
        hex::encode(sha2::Sha256::digest(include_bytes!(
            "../../../contracts/knowledge-shard/2.0.0/field-semantics.json"
        )))
    );
    assert_eq!(receipt["authority"]["fieldInventoryCount"], 220);
    assert_eq!(
        receipt["implementation"]["commit"],
        "336df3ed834be581d1a0f0a3d252fb48e723b987"
    );
    assert_eq!(
        receipt["implementation"]["deliveredMain"]["ciUrl"],
        "https://git.integrolabs.net/Fortemi/fortemi/actions/runs/5688"
    );
    assert_eq!(
        receipt["implementation"]["deliveredMain"]["testCiUrl"],
        "https://git.integrolabs.net/Fortemi/fortemi/actions/runs/5688"
    );
    assert_eq!(
        receipt["implementation"]["deliveredMain"]["conclusion"],
        "success"
    );
    assert_eq!(receipt["reactProducer"]["package"]["version"], "2026.7.13");
    assert_eq!(
        receipt["reactProducer"]["package"]["integrity"],
        "sha512-bFf77/wQhJ9M9m/0M3TM1S13EkmfrBM/O5sVaTkXJaeo1uyCuvP46T9ZVm3pGae30AkpWI3fDxuwo2AvEOBKOw=="
    );
    assert_eq!(
        receipt["reactProducer"]["archive"]["sha256"],
        hex::encode(sha2::Sha256::digest(include_bytes!(
            "../../../tests/fixtures/shards/external/react-2026.7.13/react-full-v1.shard"
        )))
    );
    assert_eq!(
        receipt["aiwgProducer"]["fixtureCommit"],
        "7ebc5c23929650e9cc762b9f5831be113fffbae8"
    );
    assert_eq!(
        receipt["aiwgProducer"]["deliveredMainCommit"],
        "bebe053df4892d9ae5cced822aead3d9d7f19656"
    );
    assert_eq!(
        receipt["aiwgProducer"]["archive"]["sha256"],
        hex::encode(sha2::Sha256::digest(include_bytes!(
            "../../../tests/fixtures/shards/external/aiwg-2026.7.13/aiwg-full-v1.shard"
        )))
    );
    assert_eq!(receipt["consumer"]["repeatedImports"], 2);
    assert_eq!(receipt["consumer"]["zeroMutationOnFailure"], true);
    assert_eq!(
        receipt["implementation"]["immutableSidecar"]["assetSha256"],
        "db89a3e52323e67f66e0c6b446667dce88349228882c4919a8f821c75bbfaad6"
    );
    assert_eq!(
        receipt["productionSigning"]["privateMaterialSerialized"],
        false
    );
    assert_eq!(
        receipt["productionSigning"]["hotmLiveReceipt"]["commit"],
        "234215e6754b1bcc22c1a7449b6f6b498aee07c5"
    );
    assert_eq!(
        receipt["productionSigning"]["hotmLiveReceipt"]["sha256"],
        "efd8516925f7c5fea8d59d98644827ce5ca9a8a2ecd7bb3dd0700c26f8abe1f2"
    );
    assert_eq!(
        receipt["productionSigning"]["hotmLiveReceipt"]["status"],
        "passed"
    );
    assert_eq!(receipt["advertisement"]["advertised"], false);
}
