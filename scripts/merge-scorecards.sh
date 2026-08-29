#!/usr/bin/env bash
set -euo pipefail

INPUT=$(cat)

if ! echo "$INPUT" | jq empty 2>/dev/null; then
  echo "Error: invalid JSON input" >&2
  exit 2
fi

INPUT_TYPE=$(echo "$INPUT" | jq -r 'type')
if [ "$INPUT_TYPE" != "array" ]; then
  echo "Error: input must be a JSON array" >&2
  exit 2
fi

INPUT_LEN=$(echo "$INPUT" | jq 'length')
if [ "$INPUT_LEN" -eq 0 ]; then
  echo "Error: input array must not be empty" >&2
  exit 2
fi

if ! echo "$INPUT" | jq -e 'all(.[]; type == "object" and (.criteria | type == "array"))' >/dev/null 2>&1; then
  echo '{"error": "every scorecard must contain a criteria array"}' >&2
  exit 2
fi

echo "$INPUT" | jq -c '
  . as $cards |

  [.[] | .domains | keys[]] | unique as $all_domains |

  ($all_domains | map(. as $domain |
    {
      ($domain): {
        "dimensions": (
          [$cards[] | .domains[$domain] // {} | .dimensions // {} | keys[]] | unique |
          map(. as $dim |
            ($cards | map(
              select(.domains[$domain] != null and .domains[$domain].dimensions[$dim] != null) |
              {(.provider): .domains[$domain].dimensions[$dim].score}
            ) | add) as $provider_scores |

            ($cards | map(
              select(.domains[$domain] != null and .domains[$domain].dimensions[$dim] != null) |
              .domains[$domain].dimensions[$dim].threshold
            ) | first) as $threshold |

            ([$provider_scores | to_entries[].value] | min) as $min_score |

            {
              ($dim): {
                "score": $min_score,
                "threshold": $threshold,
                "provider_scores": $provider_scores
              }
            }
          ) | add // {}
        )
      }
    }
  ) | add // {}) as $merged_domains |

  ([$cards[].criteria[]?] | group_by(.id) | map(
    {
      "id": .[0].id,
      "pass": (all(.pass)),
      "evidence": .[0].evidence
    }
  )) as $merged_criteria |

  ([
    $cards[] as $card |
    $card.spec_coverage_matrix[]? |
    . + {"source_provider": $card.provider}
  ] | group_by(.requirement) | map(
    . as $entries |
    ($entries | min_by(
      if .coverage == "Missing" then 0
      elif .coverage == "Weak" then 1
      elif .coverage == "Full" then 2
      else 3
      end
    )) as $conservative |
    ($conservative + {
      "provider_coverage": (
        $entries | map({(.source_provider): .coverage}) | add
      )
    }) | del(.source_provider)
  )) as $merged_coverage |

  ([
    $cards[] as $card |
    $card.remediation_beans[]? |
    . + {"source_provider": $card.provider}
  ] | group_by(.requirement) | map(
    . as $entries |
    ($entries | max_by((.description // "") | length)) as $specific |
    ($specific + {
      "source_providers": ([$entries[].source_provider] | unique)
    }) | del(.source_provider)
  )) as $merged_remediation |

  ([
    $cards[] |
    select(
      (.spec_defect == null and (has("spec_defect") | not)) or
      ((.spec_defect | type) == "object" and (.spec_defect.detected | type) != "boolean")
    ) |
    (.provider // "unnamed")
  ]) as $spec_defect_silent |

  ([
    $cards[] |
    select(
      (.spec_defect == null and has("spec_defect")) or
      ((.spec_defect | type) == "object" and (.spec_defect.detected | type) == "boolean")
    ) |
    (.provider // "unnamed")
  ]) as $spec_defect_reported |

  ([
    $cards[] |
    select((.spec_defect | type) == "object" and .spec_defect.detected == true) |
    {
      "provider": (.provider // "unnamed"),
      "domain": (.domains | keys | join(",")),
      "reason": (.spec_defect.reason // "")
    }
  ]) as $spec_defect_sources |

  ({
    "sources": $spec_defect_sources,
    "reported_by": $spec_defect_reported,
    "missing_from": $spec_defect_silent
  } |
  if ($spec_defect_sources | length) > 0 then
    . + {
      "state": "detected",
      "detected": true,
      "reason": ([$spec_defect_sources[] | "\(.domain)/\(.provider): \(.reason)"] | join(" | "))
    }
  elif ($spec_defect_silent | length) > 0 then
    . + { "state": "not_reported" }
  else
    . + { "state": "clear", "detected": false }
  end) as $merged_spec_defect |

  {
    "task_id": $cards[0].task_id,
    "iteration": $cards[0].iteration,
    "timestamp": $cards[0].timestamp,
    "domains": $merged_domains,
    "criteria": $merged_criteria,
    "antipatterns_detected": ([$cards[].antipatterns_detected[]?] | unique),
    "spec_defect": $merged_spec_defect,
    "guidance": ([$cards[].guidance // empty] | join("\n---\n")),
    "dispatch_count": ([$cards[].dispatch_count // 0] | add)
  } |
  if ($cards | all(.[]; .mode == "evidence-only")) then .mode = "evidence-only" else . end |
  if ($cards | any(.[]; has("spec_coverage_matrix"))) then
    .spec_coverage_matrix = $merged_coverage
  else . end |
  if ($cards | any(.[]; has("remediation_beans"))) then
    .remediation_beans = $merged_remediation
  else . end
'

echo "$INPUT" | jq -c '
  . as $cards |
  [.[] | .domains | keys[]] | unique as $all_domains |

  [
    $all_domains[] | . as $domain |
    ([$cards[] | .domains[$domain] // {} | .dimensions // {} | keys[]] | unique)[] | . as $dim |
    (
      $cards | map(
        select(.domains[$domain] != null and .domains[$domain].dimensions[$dim] != null) |
        {(.provider): .domains[$domain].dimensions[$dim].score}
      ) | add
    ) as $provider_scores |
    ([$provider_scores | to_entries[].value] | max) as $max |
    ([$provider_scores | to_entries[].value] | min) as $min |
    ($max - $min) as $spread |
    select($spread >= 3) |
    {
      "domain": $domain,
      "dimension": $dim,
      "spread": $spread,
      "scores": $provider_scores
    }
  ]
' >&2
