-- Aggregate-only preflight for the Fortemi tag-path contract.
--
-- This query deliberately returns counts rather than tag names because tag
-- taxonomy may contain customer or secret-derived content. It performs no
-- writes and is safe to run before considering a stricter database constraint
-- or normalization migration.

WITH classified AS (
    SELECT
        name,
        name = '' AS is_empty,
        char_length(name) > 100 AS is_too_long,
        cardinality(string_to_array(name, '/')) > 5 AS is_too_deep,
        name LIKE '/%'
            OR name LIKE '%/'
            OR strpos(name, '//') > 0 AS has_empty_component,
        name !~ '^[[:alnum:]_/-]+$' AS has_invalid_character
    FROM tag
), case_collisions AS (
    SELECT lower(name) AS folded_name
    FROM tag
    GROUP BY lower(name)
    HAVING count(*) > 1
)
SELECT
    count(*) AS total_tags,
    count(*) FILTER (WHERE is_empty) AS empty_names,
    count(*) FILTER (WHERE is_too_long) AS overlong_names,
    count(*) FILTER (WHERE is_too_deep) AS overdeep_paths,
    count(*) FILTER (WHERE has_empty_component) AS empty_component_paths,
    count(*) FILTER (WHERE has_invalid_character) AS invalid_character_names,
    (SELECT count(*) FROM case_collisions) AS case_collision_groups
FROM classified;

-- Relationship impact for every tag that is outside the current contract.
-- This remains aggregate-only: neither tag names nor note identifiers leave
-- the database session.
WITH classified AS (
    SELECT
        name,
        name = '' AS is_empty,
        char_length(name) > 100 AS is_too_long,
        cardinality(string_to_array(name, '/')) > 5 AS is_too_deep,
        name LIKE '/%'
            OR name LIKE '%/'
            OR strpos(name, '//') > 0 AS has_empty_component,
        name !~ '^[[:alnum:]_/-]+$' AS has_invalid_character
    FROM tag
), impacted AS (
    SELECT
        classified.name,
        classified.is_too_long,
        classified.has_invalid_character,
        count(note_tag.note_id) AS relationship_count
    FROM classified
    LEFT JOIN note_tag ON note_tag.tag_name = classified.name
    WHERE classified.is_empty
        OR classified.is_too_long
        OR classified.is_too_deep
        OR classified.has_empty_component
        OR classified.has_invalid_character
    GROUP BY
        classified.name,
        classified.is_too_long,
        classified.has_invalid_character
)
SELECT
    count(*) AS incompatible_tags,
    count(*) FILTER (
        WHERE is_too_long AND has_invalid_character
    ) AS overlapping_findings,
    coalesce(sum(relationship_count), 0) AS impacted_note_tag_relationships,
    count(*) FILTER (
        WHERE relationship_count = 0
    ) AS unattached_incompatible_tags
FROM impacted;
