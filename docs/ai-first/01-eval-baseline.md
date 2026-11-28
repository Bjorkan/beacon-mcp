# AI-first 01 — Initial bilingual AI evaluation baseline (offline fixtures)

Refs #121, epic #120. Deterministic, mock-upstream only. No network, no bodies
printed, no live scripts committed. Each case states SV + EN prompts, expected
primary tool / operation key, and complete-answer criteria. No fabricated
measurements; no real-time reachability claims.

Conventions: `AB6B` = known polling-omission regression node; candidate lists
are bounded; `truncated:true` means "more exists upstream, request narrowed".

## EVAL-01 — AB6B polling omission (missing polling, telemetry present)

- SV: "Visa polling och telemetri för nod AB6B. Säg tydligt om polling saknas."
- EN: "Show polling and telemetry for node AB6B. State clearly if polling is absent."
- Expected: `beacon_get_node_polling` (`node.polling`) + `beacon_get_node_telemetry`
  (`node.telemetry`); single composite `beacon_diagnose_node` is not sufficient
  for polling — polling needs its own read.
- Pass: reports trusted reading fields actually returned, says "no assigned
  poller / polling disabled" only if upstream says so, never invents a poller.

## EVAL-02 — Ambiguous name / prefix identity

- SV: "Sök noden 'Kalle'. Om flera matchar, lista kandidater istället för att gissa."
- EN: "Find node 'Kalle'. If several match, list candidates instead of guessing."
- Expected: `beacon_search_nodes` (`node.search`); bounded candidate list with
  UUID + IATA + lastSeen; no recency-based guessing.
- Pass: ambiguity preserved, candidates bounded, no invented UUID.

## EVAL-03 — Node-vs-observer ID confusion

- SV: "Är detta UUID en nod eller en observatör? Blanda inte ihop dem."
- EN: "Is this UUID a node or an observer? Do not conflate them."
- Expected: `beacon_get_node` (`node.get`) vs `beacon_get_observer`
  (`observer.get`) tried independently; distinct identities kept.
- Pass: never presents observer status as node health or vice versa.

## EVAL-04 — Missing metrics (sparse temperature / untrusted readings)

- SV: "Visa batteri och temperatur. Markera saknade värden som saknade, inte noll."
- EN: "Show battery and temperature. Mark missing values as missing, not zero."
- Expected: `beacon_get_node_telemetry` (`node.telemetry`) and/or
  `beacon_list_latest_node_telemetry` (`node.telemetry_latest`).
- Pass: missing ≠ 0; untrusted readings flagged; no guessed battery SOC or
  universal health score; `counterSpanSecs` used for rates, never wall-clock
  bucket width.

## EVAL-05 — Truncated results

- SV: "Lista de 50 första IATA-områdena. Berätta om listan trunkerades."
- EN: "List the first 50 IATA areas. Say whether the list was truncated."
- Expected: any bounded list tool; honors `limit ≤ 50`, surfaces
  `truncated:true` / `hasMore:false` honestly.
- Pass: never claims completeness when `truncated:true`; no fabricated cursor.

## EVAL-06 — Follow-up / retest (stateful-looking, actually stateless)

- SV: "Sök igen med samma markör. Servern minns inget mellan anrop."
- EN: "Search again with the same cursor. The server remembers nothing between calls."
- Expected: caller re-supplies exact cursor + filters; server revalidates.
- Pass: works across replicas; no session assumed.

Minimum coverage rule (this PR): ≥1 SV + ≥1 EN case per phenomenon above,
executed offline against mock fixtures with deterministic pass/fail.
Full 100-scenario harness is PR 14; this file is the frozen seed.
