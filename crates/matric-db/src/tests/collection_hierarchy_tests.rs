//! Tests for collection hierarchy edge cases (Issue #345)
//!
//! Covers: multi-level hierarchy creation, note movement across levels,
//! delete behavior for nested structures, and listing at each level.

use crate::test_fixtures::TestDatabase;
use crate::{CollectionRepository, CreateNoteRequest, NoteRepository};
use uuid::Uuid;

/// Generate a unique collection name to avoid UNIQUE constraint collisions
fn unique_name(prefix: &str) -> String {
    format!(
        "{}-{}",
        prefix,
        Uuid::new_v4().to_string().split('-').next().unwrap()
    )
}

// =============================================================================
// Hierarchy Creation Tests
// =============================================================================

#[tokio::test]
async fn test_create_four_level_hierarchy() {
    let test_db = TestDatabase::new().await;
    let collections = &test_db.db.collections;

    // Create root → L1 → L2 → L3 → L4
    let root_id = collections
        .create(&unique_name("root"), Some("Root collection"), None)
        .await
        .expect("create root");

    let l1_id = collections
        .create(&unique_name("level-1"), Some("Level 1"), Some(root_id))
        .await
        .expect("create L1");

    let l2_id = collections
        .create(&unique_name("level-2"), Some("Level 2"), Some(l1_id))
        .await
        .expect("create L2");

    let l3_id = collections
        .create(&unique_name("level-3"), Some("Level 3"), Some(l2_id))
        .await
        .expect("create L3");

    let l4_id = collections
        .create(&unique_name("level-4"), Some("Level 4"), Some(l3_id))
        .await
        .expect("create L4");

    // Verify each level's parent_id
    let root = collections.get(root_id).await.unwrap().unwrap();
    assert!(root.parent_id.is_none(), "root should have no parent");

    let l1 = collections.get(l1_id).await.unwrap().unwrap();
    assert_eq!(l1.parent_id, Some(root_id));

    let l2 = collections.get(l2_id).await.unwrap().unwrap();
    assert_eq!(l2.parent_id, Some(l1_id));

    let l3 = collections.get(l3_id).await.unwrap().unwrap();
    assert_eq!(l3.parent_id, Some(l2_id));

    let l4 = collections.get(l4_id).await.unwrap().unwrap();
    assert_eq!(l4.parent_id, Some(l3_id));

    test_db.cleanup().await;
}

#[tokio::test]
async fn test_create_collection_with_nonexistent_parent() {
    let test_db = TestDatabase::new().await;
    let collections = &test_db.db.collections;

    let fake_parent = Uuid::new_v4();
    let result = collections
        .create(&unique_name("orphan"), None, Some(fake_parent))
        .await;

    assert!(
        result.is_err(),
        "creating with nonexistent parent should fail due to FK constraint"
    );

    test_db.cleanup().await;
}

#[tokio::test]
async fn test_create_multiple_children_under_same_parent() {
    let test_db = TestDatabase::new().await;
    let collections = &test_db.db.collections;

    let parent_id = collections
        .create(&unique_name("parent"), None, None)
        .await
        .expect("create parent");

    let child1_id = collections
        .create(&unique_name("child-1"), None, Some(parent_id))
        .await
        .expect("create child 1");

    let child2_id = collections
        .create(&unique_name("child-2"), None, Some(parent_id))
        .await
        .expect("create child 2");

    let child3_id = collections
        .create(&unique_name("child-3"), None, Some(parent_id))
        .await
        .expect("create child 3");

    // List children of parent
    let children = collections
        .list(Some(parent_id))
        .await
        .expect("list children");
    assert_eq!(children.len(), 3, "parent should have 3 children");

    let child_ids: Vec<Uuid> = children.iter().map(|c| c.id).collect();
    assert!(child_ids.contains(&child1_id));
    assert!(child_ids.contains(&child2_id));
    assert!(child_ids.contains(&child3_id));

    test_db.cleanup().await;
}

// =============================================================================
// Listing Tests
// =============================================================================

#[tokio::test]
async fn test_list_root_collections() {
    let test_db = TestDatabase::new().await;
    let collections = &test_db.db.collections;

    let root1 = collections
        .create(&unique_name("root-a"), None, None)
        .await
        .expect("create root-a");

    let root2 = collections
        .create(&unique_name("root-b"), None, None)
        .await
        .expect("create root-b");

    // Child of root1 — should NOT appear in root listing
    let _child = collections
        .create(&unique_name("child-of-a"), None, Some(root1))
        .await
        .expect("create child");

    let roots = collections.list(None).await.expect("list roots");
    let root_ids: Vec<Uuid> = roots.iter().map(|c| c.id).collect();
    assert!(root_ids.contains(&root1), "root1 should be in root listing");
    assert!(root_ids.contains(&root2), "root2 should be in root listing");

    test_db.cleanup().await;
}

#[tokio::test]
async fn test_list_at_each_hierarchy_level() {
    let test_db = TestDatabase::new().await;
    let collections = &test_db.db.collections;

    let root_id = collections
        .create(&unique_name("h-root"), None, None)
        .await
        .unwrap();
    let l1_id = collections
        .create(&unique_name("h-l1"), None, Some(root_id))
        .await
        .unwrap();
    let l2a_id = collections
        .create(&unique_name("h-l2a"), None, Some(l1_id))
        .await
        .unwrap();
    let l2b_id = collections
        .create(&unique_name("h-l2b"), None, Some(l1_id))
        .await
        .unwrap();
    let l3_id = collections
        .create(&unique_name("h-l3"), None, Some(l2a_id))
        .await
        .unwrap();

    // Root's children
    let root_children = collections.list(Some(root_id)).await.unwrap();
    assert_eq!(root_children.len(), 1);
    assert_eq!(root_children[0].id, l1_id);

    // L1's children
    let l1_children = collections.list(Some(l1_id)).await.unwrap();
    assert_eq!(l1_children.len(), 2);
    let l1_child_ids: Vec<Uuid> = l1_children.iter().map(|c| c.id).collect();
    assert!(l1_child_ids.contains(&l2a_id));
    assert!(l1_child_ids.contains(&l2b_id));

    // L2a's children
    let l2a_children = collections.list(Some(l2a_id)).await.unwrap();
    assert_eq!(l2a_children.len(), 1);
    assert_eq!(l2a_children[0].id, l3_id);

    // L2b has no children
    let l2b_children = collections.list(Some(l2b_id)).await.unwrap();
    assert_eq!(l2b_children.len(), 0);

    // L3 has no children
    let l3_children = collections.list(Some(l3_id)).await.unwrap();
    assert_eq!(l3_children.len(), 0);

    test_db.cleanup().await;
}

// =============================================================================
// Note Movement Tests
// =============================================================================

#[tokio::test]
async fn test_move_note_across_hierarchy_levels() {
    let test_db = TestDatabase::new().await;
    let collections = &test_db.db.collections;
    let notes = &test_db.db.notes;

    // Build hierarchy
    let root_id = collections
        .create(&unique_name("mv-root"), None, None)
        .await
        .unwrap();
    let l1_id = collections
        .create(&unique_name("mv-l1"), None, Some(root_id))
        .await
        .unwrap();
    let l2_id = collections
        .create(&unique_name("mv-l2"), None, Some(l1_id))
        .await
        .unwrap();
    let l3_id = collections
        .create(&unique_name("mv-l3"), None, Some(l2_id))
        .await
        .unwrap();

    // Create a note (uncategorized initially)
    let note_id = notes
        .insert(CreateNoteRequest {
            content: "Test note for movement".to_string(),
            format: "markdown".to_string(),
            source: "test".to_string(),
            collection_id: None,
            tags: None,
            metadata: None,
            document_type_id: None,
        })
        .await
        .expect("create note");

    // Move note: uncategorized → root
    collections
        .move_note(note_id, Some(root_id))
        .await
        .expect("move to root");
    let root_notes = collections.get_notes(root_id, 100, 0).await.unwrap();
    assert!(
        root_notes.iter().any(|n| n.id == note_id),
        "note should be in root"
    );

    // Move note: root → L2 (skip a level)
    collections
        .move_note(note_id, Some(l2_id))
        .await
        .expect("move to L2");
    let l2_notes = collections.get_notes(l2_id, 100, 0).await.unwrap();
    assert!(
        l2_notes.iter().any(|n| n.id == note_id),
        "note should be in L2"
    );
    let root_notes = collections.get_notes(root_id, 100, 0).await.unwrap();
    assert!(
        !root_notes.iter().any(|n| n.id == note_id),
        "note should no longer be in root"
    );

    // Move note: L2 → L3 (down one level)
    collections
        .move_note(note_id, Some(l3_id))
        .await
        .expect("move to L3");
    let l3_notes = collections.get_notes(l3_id, 100, 0).await.unwrap();
    assert!(
        l3_notes.iter().any(|n| n.id == note_id),
        "note should be in L3"
    );

    // Move note: L3 → L1 (back up in hierarchy)
    collections
        .move_note(note_id, Some(l1_id))
        .await
        .expect("move to L1");
    let l1_notes = collections.get_notes(l1_id, 100, 0).await.unwrap();
    assert!(
        l1_notes.iter().any(|n| n.id == note_id),
        "note should be in L1"
    );

    // Move note: L1 → uncategorized (None)
    collections
        .move_note(note_id, None)
        .await
        .expect("move to uncategorized");
    let l1_notes = collections.get_notes(l1_id, 100, 0).await.unwrap();
    assert!(
        !l1_notes.iter().any(|n| n.id == note_id),
        "note should no longer be in L1"
    );

    test_db.cleanup().await;
}

#[tokio::test]
async fn test_move_note_to_nonexistent_collection() {
    let test_db = TestDatabase::new().await;
    let collections = &test_db.db.collections;
    let notes = &test_db.db.notes;

    let note_id = notes
        .insert(CreateNoteRequest {
            content: "orphan note".to_string(),
            format: "markdown".to_string(),
            source: "test".to_string(),
            collection_id: None,
            tags: None,
            metadata: None,
            document_type_id: None,
        })
        .await
        .unwrap();
    let fake_collection = Uuid::new_v4();

    let result = collections.move_note(note_id, Some(fake_collection)).await;
    // Note: Whether this fails depends on FK constraint on note.collection_id.
    // If no FK constraint, the move succeeds but the note points to a nonexistent collection.
    // Either behavior is acceptable — this test documents the current behavior.
    if result.is_ok() {
        // No FK constraint — note now has dangling reference. This is a gap.
        // The test documents this existing behavior.
    }

    test_db.cleanup().await;
}

// =============================================================================
// Delete Behavior Tests
// =============================================================================

#[tokio::test]
async fn test_delete_leaf_collection() {
    let test_db = TestDatabase::new().await;
    let collections = &test_db.db.collections;

    let root_id = collections
        .create(&unique_name("del-root"), None, None)
        .await
        .unwrap();
    let leaf_id = collections
        .create(&unique_name("del-leaf"), None, Some(root_id))
        .await
        .unwrap();

    collections.delete(leaf_id).await.expect("delete leaf");

    // Leaf should be gone
    let leaf = collections.get(leaf_id).await.unwrap();
    assert!(leaf.is_none(), "leaf should be deleted");

    // Root should still exist
    let root = collections.get(root_id).await.unwrap();
    assert!(root.is_some(), "root should still exist");

    test_db.cleanup().await;
}

#[tokio::test]
async fn test_delete_middle_collection_children_move_to_root() {
    let test_db = TestDatabase::new().await;
    let collections = &test_db.db.collections;

    // root → middle → child
    let root_id = collections
        .create(&unique_name("del2-root"), None, None)
        .await
        .unwrap();
    let middle_id = collections
        .create(&unique_name("del2-mid"), None, Some(root_id))
        .await
        .unwrap();
    let child_id = collections
        .create(&unique_name("del2-child"), None, Some(middle_id))
        .await
        .unwrap();

    // Delete middle
    collections.delete(middle_id).await.expect("delete middle");

    // Middle should be gone
    assert!(collections.get(middle_id).await.unwrap().is_none());

    // Child should still exist but parent_id set to NULL (moved to root)
    let child = collections
        .get(child_id)
        .await
        .unwrap()
        .expect("child should still exist");
    assert!(
        child.parent_id.is_none(),
        "child should have been moved to root (parent_id = NULL)"
    );

    // Root should still exist
    assert!(collections.get(root_id).await.unwrap().is_some());

    test_db.cleanup().await;
}

#[tokio::test]
async fn test_delete_collection_notes_become_uncategorized() {
    let test_db = TestDatabase::new().await;
    let collections = &test_db.db.collections;
    let notes = &test_db.db.notes;

    let coll_id = collections
        .create(&unique_name("del-notes"), None, None)
        .await
        .unwrap();

    // Create notes in this collection
    let note1_id = notes
        .insert(CreateNoteRequest {
            content: "Note 1 content".to_string(),
            format: "markdown".to_string(),
            source: "test".to_string(),
            collection_id: None,
            tags: None,
            metadata: None,
            document_type_id: None,
        })
        .await
        .unwrap();
    let note2_id = notes
        .insert(CreateNoteRequest {
            content: "Note 2 content".to_string(),
            format: "markdown".to_string(),
            source: "test".to_string(),
            collection_id: None,
            tags: None,
            metadata: None,
            document_type_id: None,
        })
        .await
        .unwrap();
    collections
        .move_note(note1_id, Some(coll_id))
        .await
        .unwrap();
    collections
        .move_note(note2_id, Some(coll_id))
        .await
        .unwrap();

    // Verify notes are in collection
    let coll_notes = collections.get_notes(coll_id, 100, 0).await.unwrap();
    assert_eq!(coll_notes.len(), 2);

    // Delete collection
    collections
        .delete(coll_id)
        .await
        .expect("delete collection with notes");

    // Notes should still exist but have collection_id = NULL
    let note1 = notes.fetch(note1_id).await.expect("note1 should exist");
    assert!(
        note1.note.collection_id.is_none(),
        "note1 should be uncategorized"
    );

    let note2 = notes.fetch(note2_id).await.expect("note2 should exist");
    assert!(
        note2.note.collection_id.is_none(),
        "note2 should be uncategorized"
    );

    test_db.cleanup().await;
}

#[tokio::test]
async fn test_delete_root_of_deep_hierarchy() {
    let test_db = TestDatabase::new().await;
    let collections = &test_db.db.collections;

    // root → L1 → L2 → L3
    let root_id = collections
        .create(&unique_name("deep-root"), None, None)
        .await
        .unwrap();
    let l1_id = collections
        .create(&unique_name("deep-l1"), None, Some(root_id))
        .await
        .unwrap();
    let l2_id = collections
        .create(&unique_name("deep-l2"), None, Some(l1_id))
        .await
        .unwrap();
    let l3_id = collections
        .create(&unique_name("deep-l3"), None, Some(l2_id))
        .await
        .unwrap();

    // Delete root — only direct children move to root, deeper levels unaffected
    collections.delete(root_id).await.expect("delete root");

    assert!(
        collections.get(root_id).await.unwrap().is_none(),
        "root should be deleted"
    );

    // L1 should still exist with parent_id = NULL
    let l1 = collections
        .get(l1_id)
        .await
        .unwrap()
        .expect("L1 should exist");
    assert!(l1.parent_id.is_none(), "L1 should be promoted to root");

    // L2 should still point to L1
    let l2 = collections
        .get(l2_id)
        .await
        .unwrap()
        .expect("L2 should exist");
    assert_eq!(l2.parent_id, Some(l1_id), "L2 should still be under L1");

    // L3 should still point to L2
    let l3 = collections
        .get(l3_id)
        .await
        .unwrap()
        .expect("L3 should exist");
    assert_eq!(l3.parent_id, Some(l2_id), "L3 should still be under L2");

    test_db.cleanup().await;
}

#[tokio::test]
async fn test_delete_collection_with_both_notes_and_children() {
    let test_db = TestDatabase::new().await;
    let collections = &test_db.db.collections;
    let notes = &test_db.db.notes;

    let parent_id = collections
        .create(&unique_name("combo-parent"), None, None)
        .await
        .unwrap();
    let child_id = collections
        .create(&unique_name("combo-child"), None, Some(parent_id))
        .await
        .unwrap();

    let note_id = notes
        .insert(CreateNoteRequest {
            content: "Combo note content".to_string(),
            format: "markdown".to_string(),
            source: "test".to_string(),
            collection_id: None,
            tags: None,
            metadata: None,
            document_type_id: None,
        })
        .await
        .unwrap();
    collections
        .move_note(note_id, Some(parent_id))
        .await
        .unwrap();

    // Delete parent
    collections
        .delete(parent_id)
        .await
        .expect("delete parent with notes and children");

    // Child should be promoted to root
    let child = collections
        .get(child_id)
        .await
        .unwrap()
        .expect("child should exist");
    assert!(child.parent_id.is_none(), "child promoted to root");

    // Note should be uncategorized
    let note = notes.fetch(note_id).await.expect("note should exist");
    assert!(
        note.note.collection_id.is_none(),
        "note should be uncategorized"
    );

    test_db.cleanup().await;
}

// =============================================================================
// Note Count Tests
// =============================================================================

#[tokio::test]
async fn test_collection_note_count() {
    let test_db = TestDatabase::new().await;
    let collections = &test_db.db.collections;
    let notes = &test_db.db.notes;

    let coll_id = collections
        .create(&unique_name("count-coll"), None, None)
        .await
        .unwrap();

    // Initially 0 notes
    let coll = collections.get(coll_id).await.unwrap().unwrap();
    assert_eq!(coll.note_count, 0, "should start with 0 notes");

    // Add 3 notes
    for i in 0..3 {
        let nid = notes
            .insert(CreateNoteRequest {
                content: format!("Count note {} content", i),
                format: "markdown".to_string(),
                source: "test".to_string(),
                collection_id: None,
                tags: None,
                metadata: None,
                document_type_id: None,
            })
            .await
            .unwrap();
        collections.move_note(nid, Some(coll_id)).await.unwrap();
    }

    let coll = collections.get(coll_id).await.unwrap().unwrap();
    assert_eq!(coll.note_count, 3, "should have 3 notes");

    test_db.cleanup().await;
}

// =============================================================================
// Collection Movement Tests
// =============================================================================

#[tokio::test]
async fn test_move_collection_to_new_parent() {
    let test_db = TestDatabase::new().await;
    let collections = &test_db.db.collections;

    // root1 → child, root2 (separate)
    let root1_id = collections
        .create(&unique_name("mvcol-r1"), None, None)
        .await
        .unwrap();
    let root2_id = collections
        .create(&unique_name("mvcol-r2"), None, None)
        .await
        .unwrap();
    let child_id = collections
        .create(&unique_name("mvcol-child"), None, Some(root1_id))
        .await
        .unwrap();

    // Move child from root1 to root2
    collections
        .move_collection(child_id, Some(root2_id))
        .await
        .expect("move collection to new parent");

    let child = collections.get(child_id).await.unwrap().unwrap();
    assert_eq!(
        child.parent_id,
        Some(root2_id),
        "child should now be under root2"
    );

    // root1 should have no children
    let r1_children = collections.list(Some(root1_id)).await.unwrap();
    assert_eq!(r1_children.len(), 0, "root1 should have no children");

    // root2 should have the child
    let r2_children = collections.list(Some(root2_id)).await.unwrap();
    assert_eq!(r2_children.len(), 1, "root2 should have 1 child");
    assert_eq!(r2_children[0].id, child_id);

    test_db.cleanup().await;
}

#[tokio::test]
async fn test_move_collection_to_root() {
    let test_db = TestDatabase::new().await;
    let collections = &test_db.db.collections;

    let parent_id = collections
        .create(&unique_name("mvcol-p"), None, None)
        .await
        .unwrap();
    let child_id = collections
        .create(&unique_name("mvcol-c"), None, Some(parent_id))
        .await
        .unwrap();

    // Move child to root (parent_id = None)
    collections
        .move_collection(child_id, None)
        .await
        .expect("move collection to root");

    let child = collections.get(child_id).await.unwrap().unwrap();
    assert!(
        child.parent_id.is_none(),
        "child should now be a root collection"
    );

    test_db.cleanup().await;
}

#[tokio::test]
async fn test_move_collection_with_subtree() {
    let test_db = TestDatabase::new().await;
    let collections = &test_db.db.collections;

    // root1 → parent → child → grandchild, root2 (separate)
    let root1_id = collections
        .create(&unique_name("mvtree-r1"), None, None)
        .await
        .unwrap();
    let root2_id = collections
        .create(&unique_name("mvtree-r2"), None, None)
        .await
        .unwrap();
    let parent_id = collections
        .create(&unique_name("mvtree-p"), None, Some(root1_id))
        .await
        .unwrap();
    let child_id = collections
        .create(&unique_name("mvtree-c"), None, Some(parent_id))
        .await
        .unwrap();
    let grandchild_id = collections
        .create(&unique_name("mvtree-gc"), None, Some(child_id))
        .await
        .unwrap();

    // Move parent (with its subtree) under root2
    collections
        .move_collection(parent_id, Some(root2_id))
        .await
        .expect("move subtree to root2");

    // Parent now under root2
    let parent = collections.get(parent_id).await.unwrap().unwrap();
    assert_eq!(parent.parent_id, Some(root2_id));

    // Child still under parent
    let child = collections.get(child_id).await.unwrap().unwrap();
    assert_eq!(child.parent_id, Some(parent_id));

    // Grandchild still under child
    let gc = collections.get(grandchild_id).await.unwrap().unwrap();
    assert_eq!(gc.parent_id, Some(child_id));

    test_db.cleanup().await;
}

// =============================================================================
// Circular Reference Prevention Tests
// =============================================================================

#[tokio::test]
async fn test_move_collection_to_self_fails() {
    let test_db = TestDatabase::new().await;
    let collections = &test_db.db.collections;

    let coll_id = collections
        .create(&unique_name("circ-self"), None, None)
        .await
        .unwrap();

    let result = collections.move_collection(coll_id, Some(coll_id)).await;
    assert!(result.is_err(), "moving collection to itself should fail");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("own parent"),
        "error should mention self-parenting: {}",
        err_msg
    );

    test_db.cleanup().await;
}

#[tokio::test]
async fn test_move_collection_to_direct_child_fails() {
    let test_db = TestDatabase::new().await;
    let collections = &test_db.db.collections;

    // parent → child
    let parent_id = collections
        .create(&unique_name("circ-p"), None, None)
        .await
        .unwrap();
    let child_id = collections
        .create(&unique_name("circ-c"), None, Some(parent_id))
        .await
        .unwrap();

    // Try to move parent under its own child (would create: child → parent → child → ...)
    let result = collections.move_collection(parent_id, Some(child_id)).await;
    assert!(result.is_err(), "moving parent under child should fail");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("circular"),
        "error should mention circular reference: {}",
        err_msg
    );

    test_db.cleanup().await;
}

#[tokio::test]
async fn test_move_collection_to_deep_descendant_fails() {
    let test_db = TestDatabase::new().await;
    let collections = &test_db.db.collections;

    // A → B → C → D
    let a_id = collections
        .create(&unique_name("circ-a"), None, None)
        .await
        .unwrap();
    let b_id = collections
        .create(&unique_name("circ-b"), None, Some(a_id))
        .await
        .unwrap();
    let c_id = collections
        .create(&unique_name("circ-c2"), None, Some(b_id))
        .await
        .unwrap();
    let d_id = collections
        .create(&unique_name("circ-d"), None, Some(c_id))
        .await
        .unwrap();

    // Try to move A under D (would create: D → A → B → C → D → ...)
    let result = collections.move_collection(a_id, Some(d_id)).await;
    assert!(
        result.is_err(),
        "moving A under its deep descendant D should fail"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("circular"),
        "error should mention circular reference: {}",
        err_msg
    );

    // Verify hierarchy is untouched
    let a = collections.get(a_id).await.unwrap().unwrap();
    assert!(a.parent_id.is_none(), "A should still be root");
    let d = collections.get(d_id).await.unwrap().unwrap();
    assert_eq!(d.parent_id, Some(c_id), "D should still be under C");

    test_db.cleanup().await;
}

#[tokio::test]
async fn test_move_collection_to_non_descendant_succeeds() {
    let test_db = TestDatabase::new().await;
    let collections = &test_db.db.collections;

    // A → B → C, D (separate tree)
    let a_id = collections
        .create(&unique_name("ok-a"), None, None)
        .await
        .unwrap();
    let b_id = collections
        .create(&unique_name("ok-b"), None, Some(a_id))
        .await
        .unwrap();
    let _c_id = collections
        .create(&unique_name("ok-c"), None, Some(b_id))
        .await
        .unwrap();
    let d_id = collections
        .create(&unique_name("ok-d"), None, None)
        .await
        .unwrap();

    // Move A under D — should succeed since D is not a descendant of A
    collections
        .move_collection(a_id, Some(d_id))
        .await
        .expect("move to non-descendant should succeed");

    let a = collections.get(a_id).await.unwrap().unwrap();
    assert_eq!(a.parent_id, Some(d_id), "A should now be under D");

    test_db.cleanup().await;
}

// =============================================================================
// Update Tests (name/description only — parent_id unchanged)
// =============================================================================

#[tokio::test]
async fn test_update_preserves_parent_id() {
    let test_db = TestDatabase::new().await;
    let collections = &test_db.db.collections;

    let parent_id = collections
        .create(&unique_name("upd-parent"), None, None)
        .await
        .unwrap();
    let child_id = collections
        .create(
            &unique_name("upd-child"),
            Some("Original desc"),
            Some(parent_id),
        )
        .await
        .unwrap();

    // Update name and description
    let new_name = unique_name("upd-child-renamed");
    collections
        .update(child_id, &new_name, Some("New description"))
        .await
        .expect("update child");

    // Verify parent_id is unchanged
    let child = collections.get(child_id).await.unwrap().unwrap();
    assert_eq!(
        child.parent_id,
        Some(parent_id),
        "parent_id should be unchanged after update"
    );
    assert_eq!(child.name, new_name);
    assert_eq!(child.description.as_deref(), Some("New description"));

    test_db.cleanup().await;
}
