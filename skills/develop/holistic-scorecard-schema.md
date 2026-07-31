# Holistic Scorecard Schema

## Spec Coverage Matrix Protocol

After scoring all dimensions, produce a spec coverage matrix. Extract every
requirement from the design document and classify each:

| Coverage | Meaning |
|---|---|
| **Full** | Requirement implemented and verified via runtime evidence |
| **Weak** | Requirement partially implemented or implemented but not fully verified |
| **Missing** | Requirement not implemented or no evidence of implementation |

### Format

Produce the matrix as a JSON array in the scorecard output:

```json
{
  "spec_coverage_matrix": [
    {
      "requirement": "Radial spoke layout",
      "coverage": "Full",
      "evidence": "Screenshot shows 6 spokes radiating from center"
    },
    {
      "requirement": "Camera zoom 0.3x-2.0x",
      "coverage": "Weak",
      "evidence": "Zoom works but bounds not tested at extremes"
    },
    {
      "requirement": "Seed elements in empty districts",
      "coverage": "Missing",
      "evidence": "Not visible in any screenshot or interaction"
    }
  ]
}
```

### Rules

- Every requirement in the design document must appear in the matrix — do not skip requirements
- "Full" requires runtime evidence (screenshot, curl response, interaction log)
- "Weak" means evidence exists but is incomplete — flag for human judgment
- "Missing" means no evidence found — these become remediation tasks automatically

## Remediation Bean Generation

For each **Missing** entry in the spec coverage matrix and each holistic dimension that scores **below its threshold**, generate a remediation bean.

### Format

Produce remediation beans as a JSON array in the scorecard output:

```json
{
  "remediation_beans": [
    {
      "title": "Fix: Seed elements not visible in empty districts",
      "description": "The design spec requires seed elements to appear in empty districts to guide the user. No evidence of this feature was found during holistic review.",
      "source": "spec_coverage:Missing",
      "eval": {
        "criteria": [
          {
            "id": "seed_elements_visible",
            "description": "Empty districts display seed elements as specified in design doc",
            "threshold": 8
          }
        ]
      }
    },
    {
      "title": "Fix: Runtime Health below threshold (scored 6, needs 9)",
      "description": "Console errors present during runtime interaction. Multiple warnings on startup. Holistic reviewer observed degraded responsiveness during cross-domain flows.",
      "source": "dimension:runtime_health",
      "eval": {
        "criteria": [
          {
            "id": "runtime_clean_startup",
            "description": "Application starts with zero console errors or warnings",
            "threshold": 9
          },
          {
            "id": "runtime_responsive",
            "description": "All interactions respond without jank or delay",
            "threshold": 9
          }
        ]
      }
    }
  ]
}
```

### Rules

- Every "Missing" spec coverage entry produces exactly one remediation bean
- Every dimension below threshold produces one remediation bean (combine related issues)
- "Weak" entries do not automatically produce remediation beans — flag them for human review
- Each remediation bean must have an `eval` block with criteria specific to the gap
- The `source` field traces back to the coverage matrix entry or dimension that triggered it
- Bean titles start with "Fix:" to distinguish remediation from original tasks

## Scorecard Output

The holistic reviewer outputs a single JSON scorecard. The domain key is `holistic` and dimension keys are snake_case.

```json
{
  "domain": "holistic",
  "dimensions": {
    "integration": {
      "score": 7,
      "threshold": 7,
      "evidence": "Frontend correctly calls backend API endpoints. Data flows end-to-end for primary user flow. Error propagation works — backend 422 shows validation message in frontend. Minor: loading state inconsistent between create and update flows."
    },
    "coherence": {
      "score": 8,
      "threshold": 7,
      "evidence": "Consistent naming throughout. Navigation patterns match across domains. Visual language unified. Interaction patterns predictable."
    },
    "holistic_spec_fidelity": {
      "score": 7,
      "threshold": 8,
      "evidence": "Primary spec requirements met. Camera zoom and radial layout working. Missing: seed elements in empty districts. Weak: district zone gradients not as soft as spec describes."
    },
    "polish": {
      "score": 6,
      "threshold": 6,
      "evidence": "Loading states present. Error handling adequate. No console errors in normal flow. Empty states handled. Minor: hover states inconsistent on secondary buttons."
    },
    "runtime_health": {
      "score": 9,
      "threshold": 9,
      "evidence": "All runtimes start cleanly. Zero console errors or warnings. Frontend renders in under 2 seconds. Backend responds to all endpoints within 100ms. No memory growth observed during 5-minute interaction session."
    }
  },
  "cross_domain_integration": {
    "api_contract_compliance": "Frontend sends expected request shapes. Backend responds with parseable JSON. All status codes handled.",
    "data_flow_verified": true,
    "integration_gaps": []
  },
  "spec_coverage_matrix": [],
  "remediation_beans": []
}
```
