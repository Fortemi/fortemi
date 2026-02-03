# DATA-{NUMBER}: {DATASET_NAME}

<!-- Document Type: data-dictionary -->
<!-- Chunking: per-section -->
<!-- Version: 1.0.0 -->

## Dataset Profile

| Field | Value |
|-------|-------|
| **DATA ID** | DATA-{NUMBER} |
| **Name** | {DATASET_NAME} |
| **Version** | {VERSION} |
| **Type** | {Primary / Derived / Reference / Synthetic} |
| **Domain** | {DOMAIN} |
| **Format** | {CSV / JSON / Parquet / SQL / etc.} |
| **Size** | {SIZE} |
| **Records** | {COUNT} |
| **Date Created** | {DATE} |
| **Last Updated** | {DATE} |
| **Owner** | {OWNER} |
| **Status** | {Active / Archived / Deprecated} |

### FAIR Compliance

| Principle | Status | Implementation |
|-----------|--------|----------------|
| **Findable** | {Yes/Partial/No} | {IDENTIFIER_TYPE} |
| **Accessible** | {Open/Restricted/Closed} | {ACCESS_METHOD} |
| **Interoperable** | {Yes/Partial/No} | {STANDARDS_USED} |
| **Reusable** | {Yes/Partial/No} | {LICENSE} |

---

## Description

### Purpose

{WHAT_THIS_DATASET_IS_FOR}

### Scope

{WHAT_DATA_IS_INCLUDED_AND_EXCLUDED}

### Collection Method

{HOW_DATA_WAS_COLLECTED}

### Temporal Coverage

- **Start Date**: {DATE}
- **End Date**: {DATE or Ongoing}
- **Frequency**: {COLLECTION_FREQUENCY}

### Geographic Coverage

{GEOGRAPHIC_SCOPE}

---

## Source / Provenance

### Data Origin

| Source | Description | Contribution |
|--------|-------------|--------------|
| {SOURCE_1} | {DESCRIPTION} | {PERCENTAGE}% |
| {SOURCE_2} | {DESCRIPTION} | {PERCENTAGE}% |

### Lineage

```
{SOURCE_DATASET}
    ↓ {TRANSFORMATION_1}
{INTERMEDIATE_DATASET}
    ↓ {TRANSFORMATION_2}
{THIS_DATASET}
```

### Related Datasets

| DATA ID | Name | Relationship |
|---------|------|--------------|
| DATA-{XXX} | {NAME} | {Source / Derived / Related} |

### Collection Protocol

- **Protocol**: PROT-{XXX}
- **Instruments**: {INSTRUMENTS}
- **Standards**: {STANDARDS_FOLLOWED}

---

## Schema

### Field Definitions

| Field | Type | Description | Required | Constraints | Example |
|-------|------|-------------|----------|-------------|---------|
| {FIELD_1} | {TYPE} | {DESCRIPTION} | {Yes/No} | {CONSTRAINTS} | {EXAMPLE} |
| {FIELD_2} | {TYPE} | {DESCRIPTION} | {Yes/No} | {CONSTRAINTS} | {EXAMPLE} |
| {FIELD_3} | {TYPE} | {DESCRIPTION} | {Yes/No} | {CONSTRAINTS} | {EXAMPLE} |
| {FIELD_4} | {TYPE} | {DESCRIPTION} | {Yes/No} | {CONSTRAINTS} | {EXAMPLE} |
| {FIELD_5} | {TYPE} | {DESCRIPTION} | {Yes/No} | {CONSTRAINTS} | {EXAMPLE} |

### Data Types Reference

| Type | Format | Range | Null Handling |
|------|--------|-------|---------------|
| {TYPE_1} | {FORMAT} | {RANGE} | {NULL_VALUE} |
| {TYPE_2} | {FORMAT} | {RANGE} | {NULL_VALUE} |

### Enumerations

#### {ENUM_NAME}

| Value | Label | Description |
|-------|-------|-------------|
| {VALUE_1} | {LABEL} | {DESCRIPTION} |
| {VALUE_2} | {LABEL} | {DESCRIPTION} |

### Relationships

```
{TABLE_1}
    ├── {FIELD_A} → {TABLE_2}.{FIELD_B} (1:N)
    └── {FIELD_C} → {TABLE_3}.{FIELD_D} (N:M via {JUNCTION_TABLE})
```

---

## Sample Records

### Example 1: {EXAMPLE_DESCRIPTION}

```json
{
  "{FIELD_1}": "{VALUE}",
  "{FIELD_2}": "{VALUE}",
  "{FIELD_3}": "{VALUE}"
}
```

### Example 2: {EXAMPLE_DESCRIPTION}

```json
{
  "{FIELD_1}": "{VALUE}",
  "{FIELD_2}": "{VALUE}",
  "{FIELD_3}": "{VALUE}"
}
```

### Example 3: Edge Case - {DESCRIPTION}

```json
{
  "{FIELD_1}": "{VALUE}",
  "{FIELD_2}": null,
  "{FIELD_3}": "{EDGE_VALUE}"
}
```

---

## Quality Notes

### Data Quality Metrics

| Metric | Value | Target | Status |
|--------|-------|--------|--------|
| Completeness | {PERCENTAGE}% | {TARGET}% | {PASS/FAIL} |
| Accuracy | {PERCENTAGE}% | {TARGET}% | {PASS/FAIL} |
| Consistency | {PERCENTAGE}% | {TARGET}% | {PASS/FAIL} |
| Timeliness | {MEASURE} | {TARGET} | {PASS/FAIL} |
| Uniqueness | {PERCENTAGE}% | {TARGET}% | {PASS/FAIL} |

### Known Issues

| Issue | Affected Fields | Severity | Workaround |
|-------|-----------------|----------|------------|
| {ISSUE_1} | {FIELDS} | {HIGH/MED/LOW} | {WORKAROUND} |
| {ISSUE_2} | {FIELDS} | {HIGH/MED/LOW} | {WORKAROUND} |

### Missing Data

| Field | Missing % | Pattern | Handling |
|-------|-----------|---------|----------|
| {FIELD_1} | {PERCENTAGE}% | {PATTERN} | {HANDLING} |
| {FIELD_2} | {PERCENTAGE}% | {PATTERN} | {HANDLING} |

### Validation Rules

| Rule | Description | Implementation |
|------|-------------|----------------|
| {RULE_1} | {DESCRIPTION} | {IMPLEMENTATION} |
| {RULE_2} | {DESCRIPTION} | {IMPLEMENTATION} |

---

## Access Instructions

### Location

- **Production**: {LOCATION}
- **Development**: {LOCATION}
- **Archive**: {LOCATION}

### Access Method

```{LANGUAGE}
{ACCESS_CODE_EXAMPLE}
```

### Authentication

| Method | Credentials | Contact |
|--------|-------------|---------|
| {METHOD} | {CRED_TYPE} | {CONTACT} |

### Permissions

| Role | Access Level | Capabilities |
|------|--------------|--------------|
| {ROLE_1} | {LEVEL} | {CAPABILITIES} |
| {ROLE_2} | {LEVEL} | {CAPABILITIES} |

---

## Usage Guidelines

### Appropriate Uses

- {USE_1}
- {USE_2}

### Inappropriate Uses

- {MISUSE_1}
- {MISUSE_2}

### Citation

```
{CITATION_FORMAT}
```

### License

**License**: {LICENSE_NAME}
**Terms**: {KEY_TERMS}
**Attribution**: {ATTRIBUTION_REQUIREMENT}

---

## Related Documentation

| Document | Type | Description |
|----------|------|-------------|
| EXP-{XXX} | Experiment | {Data collection experiment} |
| PROT-{XXX} | Protocol | {Data collection protocol} |
| REF-{XXX} | Reference | {Methodology paper} |

---

## Change Log

| Version | Date | Changes | Author |
|---------|------|---------|--------|
| 1.0 | {DATE} | Initial version | {AUTHOR} |
| 1.1 | {DATE} | Added {FIELDS} | {AUTHOR} |
| 2.0 | {DATE} | Schema redesign | {AUTHOR} |

---

## Revision History

| Date | Author | Changes |
|------|--------|---------|
| {DATE} | {AUTHOR} | Dictionary created |
| {DATE} | {AUTHOR} | Updated schema |

---

**Document Classification**: Data Dictionary
**Sensitivity**: {Public / Internal / Confidential / Restricted}
**Retention Period**: {PERIOD}
