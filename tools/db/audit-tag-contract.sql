-- Aggregate-only preflight for the Fortemi tag-path contract.
--
-- This query deliberately returns counts rather than tag names because tag
-- taxonomy may contain customer or secret-derived content. It performs no
-- writes and is safe to run before considering a stricter database constraint
-- or normalization migration.

WITH classified AS (
    SELECT
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
