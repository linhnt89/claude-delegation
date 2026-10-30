# S094 Warm start doesn't weaken the sandbox
- Job: J3
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
A worker in the sandbox. Decision 020.

## Starting state
Warm start with a shared build cache enabled; a hidden scenario set exists.

## Steps
1. A stand-in worker writes to the shared cache, then tries to write the main checkout and read the hidden set.

## Passes when
- The cache write works; the main checkout write and the hidden read fail.

## If this fails
Turning on warm start lets workers touch the user's checkout or see the hidden checks.
