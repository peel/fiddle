# System

<!-- How the system actually works RIGHT NOW. Not aspirational — ground truth.
     Update after significant work ships. This is the first thing an agent
     should read before making technical suggestions or plans.
     Keep it honest. If something is a mess, say so. -->

## Overview

<!-- One paragraph. What are the main components and how do they connect?
     A sentence per component is fine. Diagram if it helps (mermaid). -->

## Components

<!-- For each major component/service/module:
     - What it does (one line)
     - Key technology choices
     - Where it runs
     - What it depends on
     Keep entries short. Link to code if helpful. -->

## Data

<!-- How data flows and where it lives.
     Databases, queues, caches, external APIs, file storage.
     Key schemas or models if they're central to understanding the system. -->

## Infrastructure

<!-- Where this runs. Cloud provider, clusters, CI/CD, monitoring.
     Enough to orient someone (or an agent) unfamiliar with the setup. -->

## Invariants

<!-- Things that MUST be true for the system to work correctly.
     Concurrency constraints, ordering guarantees, security boundaries.
     These are the things agents should never accidentally violate. -->

## Known issues

<!-- Technical debt, fragile areas, things that will bite you.
     Be specific. "The auth middleware is fragile" is less useful than
     "Auth middleware silently swallows token refresh errors — see auth/refresh.go:47" -->

---
Last reviewed: YYYY-MM-DD
