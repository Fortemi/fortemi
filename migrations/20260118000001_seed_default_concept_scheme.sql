-- Seed: Default SKOS concept scheme
INSERT INTO skos_concept_scheme (id, notation, uri, title, description, is_system)
VALUES (
    gen_uuid_v7(),
    'default',
    'https://matric.io/schemes/default',
    'Default Tags',
    'Default concept scheme for general-purpose tagging',
    TRUE
);
