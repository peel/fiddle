#!/usr/bin/env bash
# merge-scorecards.sh — Merge multiple provider scorecards into one.
# Reads JSON array of scorecards on stdin, writes merged scorecard to stdout.
# Disagreements (spread >= 3) emitted to stderr as JSON array.
# Exit 0 = merged successfully, 2 = invalid input.
set -euo pipefail

# Read all stdin
INPUT=$(cat)

# Validate: must be valid JSON
if ! echo "$INPUT" | jq empty 2>/dev/null; then
  echo "Error: invalid JSON input" >&2
  exit 2
fi

# Validate: must be an array
INPUT_TYPE=$(echo "$INPUT" | jq -r 'type')
if [ "$INPUT_TYPE" != "array" ]; then
  echo "Error: input must be a JSON array" >&2
  exit 2
fi

# Validate: must not be empty
INPUT_LEN=$(echo "$INPUT" | jq 'length')
if [ "$INPUT_LEN" -eq 0 ]; then
  echo "Error: input array must not be empty" >&2
  exit 2
fi

# Validate: every scorecard must carry a criteria array
if ! echo "$INPUT" | jq -e 'all(.[]; type == "object" and (.criteria | type == "array"))' >/dev/null 2>&1; then
  echo '{"error": "every scorecard must contain a criteria array"}' >&2
  exit 2
fi

# Perform the merge entirely in jq
echo "$INPUT" | jq -c '
  # Collect all unique domains and dimensions across all scorecards
  . as $cards |

  # Gather all domain names across all providers
  [.[] | .domains | keys[]] | unique as $all_domains |

  # For each domain, gather all dimension names across all providers
  # and compute min score, threshold, and provider_scores
  ($all_domains | map(. as $domain |
    {
      ($domain): {
        "dimensions": (
          # Collect all dimension names for this domain
          [$cards[] | .domains[$domain] // {} | .dimensions // {} | keys[]] | unique |
          map(. as $dim |
            # Gather scores per provider for this dimension
            ($cards | map(
              select(.domains[$domain] != null and .domains[$domain].dimensions[$dim] != null) |
              {(.provider): .domains[$domain].dimensions[$dim].score}
            ) | add) as $provider_scores |

            # Get threshold from first provider that has this dimension
            ($cards | map(
              select(.domains[$domain] != null and .domains[$domain].dimensions[$dim] != null) |
              .domains[$domain].dimensions[$dim].threshold
            ) | first) as $threshold |

            # Min score
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

  # Merge criteria: collect all criteria IDs, any fail = fail
  ([$cards[].criteria[]?] | group_by(.id) | map(
    {
      "id": .[0].id,
      "pass": (all(.pass)),
      "evidence": .[0].evidence
    }
  )) as $merged_criteria |

  # Build merged scorecard with metadata from first card
  {
    "task_id": $cards[0].task_id,
    "iteration": $cards[0].iteration,
    "timestamp": $cards[0].timestamp,
    "domains": $merged_domains,
    "criteria": $merged_criteria,
    "antipatterns_detected": ([$cards[].antipatterns_detected[]?] | unique),
    "guidance": ([$cards[].guidance // empty] | join("\n---\n")),
    "dispatch_count": ([$cards[].dispatch_count // 0] | add)
  }
' 2>/dev/null

# Now compute disagreements and emit to stderr
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
