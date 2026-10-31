use super::*;

#[tokio::test]
async fn graph_get_dispatch_requires_real_note_normalization_and_preserves_policy() {
    let id = Uuid::new_v4();
    let path = format!("/api/v1/graph/{id}");
    let input =
        route_policy::authorization_input_for_request(&Method::GET, &path, Some("tenant-a"))
            .unwrap();
    assert_eq!(input.resource.kind, ResourceKind::Note);
    assert_eq!(
        route_policy::backing_resource_normalizer(&input),
        Some(route_policy::BackingResourceNormalizer::Note)
    );
    assert!(hosted_policy_requires_normalized_resource(
        &RoleBasedPolicy,
        &input
    ));
    let good = normalize_graph_route_policy_input(input.clone(), |queried| async move {
        assert_eq!(queried, id);
        Ok(true)
    })
    .await;
    for (scope, allowed) in [("read", true), ("write", true), ("", false), ("mcp", false)] {
        let auth = Auth {
            principal: AuthPrincipal::ApiKey {
                key_id: Uuid::new_v4(),
                scope: scope.into(),
            },
        };
        assert_eq!(
            authorize_policy_input(&RoleBasedPolicy, &TracingSink, &auth, &good)
                .await
                .is_ok(),
            allowed
        );
    }
    for failed in [
        Ok(false),
        Err(matric_core::Error::Internal("fixture failure".into())),
    ] {
        let normalized =
            normalize_graph_route_policy_input(input.clone(), |_| async move { failed }).await;
        assert!(hosted_policy_requires_normalized_resource(
            &RoleBasedPolicy,
            &normalized
        ));
    }
    let missing = normalize_graph_route_policy_input(input.clone(), |_| async { Ok(false) }).await;
    assert!(graph_root_missing_after_scoped_lookup(&missing));
    assert_eq!(missing.resource.attrs["resource_id_normalized"], false);
    assert_eq!(missing.resource.tenant_id, input.resource.tenant_id);
    assert_eq!(missing.context.tenant_id, input.context.tenant_id);
    let generic = normalize_note_route_policy_input(input.clone(), |_| async { Ok(false) }).await;
    assert!(!graph_root_missing_after_scoped_lookup(&generic));
    for (scope, allowed) in [("read", true), ("write", true), ("", false), ("mcp", false)] {
        let auth = Auth {
            principal: AuthPrincipal::ApiKey {
                key_id: Uuid::new_v4(),
                scope: scope.into(),
            },
        };
        assert_eq!(
            authorize_policy_input(&RoleBasedPolicy, &TracingSink, &auth, &missing)
                .await
                .is_ok(),
            allowed
        );
    }
    let failed = normalize_graph_route_policy_input(missing.clone(), |_| async {
        Err(matric_core::Error::Internal("lookup failed".into()))
    })
    .await;
    assert!(!graph_root_missing_after_scoped_lookup(&failed));
    let read = Auth {
        principal: AuthPrincipal::ApiKey {
            key_id: Uuid::new_v4(),
            scope: "read".into(),
        },
    };
    assert!(
        authorize_policy_input(&RoleBasedPolicy, &TracingSink, &read, &failed)
            .await
            .is_err()
    );
    let mut wrong_tenant = missing.clone();
    wrong_tenant.resource.tenant_id = Some("tenant-b".into());
    assert!(
        authorize_policy_input(&RoleBasedPolicy, &TracingSink, &read, &wrong_tenant)
            .await
            .is_err()
    );
    let anonymous = Auth {
        principal: AuthPrincipal::Anonymous,
    };
    assert!(
        authorize_policy_input(&RoleBasedPolicy, &TracingSink, &anonymous, &missing)
            .await
            .is_err()
    );
    let malformed = route_policy::authorization_input_for_request(
        &Method::GET,
        "/api/v1/graph/not-uuid",
        Some("tenant-a"),
    )
    .unwrap();
    let malformed = normalize_graph_route_policy_input(malformed, |_| async {
        panic!("malformed UUID must never reach SQL")
    })
    .await;
    assert!(hosted_policy_requires_normalized_resource(
        &RoleBasedPolicy,
        &malformed
    ));
    let head =
        route_policy::authorization_input_for_request(&Method::HEAD, &path, Some("tenant-a"))
            .unwrap();
    assert!(route_policy::role_based_route_is_intentionally_closed(
        &head
    ));
    assert!(!graph_get_normalizer_applies(&head));
    assert!(!route_policy::hosted_tenant_transaction_ready(
        &Method::HEAD,
        &path
    ));
    assert_eq!(
        input.policy.archive_isolation,
        route_policy::ArchiveIsolation::CompleteCatalog
    );
    let mut mismatched = good;
    mismatched.resource.tenant_id = Some("tenant-b".into());
    let auth = Auth {
        principal: AuthPrincipal::ApiKey {
            key_id: Uuid::new_v4(),
            scope: "read".into(),
        },
    };
    assert!(
        authorize_policy_input(&RoleBasedPolicy, &TracingSink, &auth, &mismatched)
            .await
            .is_err()
    );
}
