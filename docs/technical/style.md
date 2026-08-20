# Writing style

All prose in this repository uses ASD-STE100 Simplified Technical English. Break a rule only when the rule removes necessary meaning. Say so when you break it.

## Rules

1. One sentence gives one idea. Use 20 words or fewer.
2. Use the active voice. Name the actor.
3. Use simple present, simple past, or simple future tense.
4. Use one word for one meaning. Do not change the word for the same thing.
5. Use no more than three words in a noun cluster.
6. Do not use slang, idioms, or figures of speech.
7. Use six sentences or fewer in a paragraph.
8. Use articles. Write "the gate", not "gate".
9. Write what is true. Do not write what is impressive.

## Citations

Quote the sentence or name the symbol. Do not cite a line number.

A line number becomes wrong when an edit adds a line above it. ADR 028 cited three lines in ADR 021. The same commit added two lines to ADR 021. All three citations became wrong.

## Where each kind of text belongs

| text | file |
| --- | --- |
| what a component is | SYSTEM.md, one entry, no paragraph |
| why a decision was made | an ADR |
| how to run something | RUNBOOKS.md, commands not prose |
| a defect or an idea | BACKLOG.md, the claim and the evidence |
| what a task requires | a bean, in the template slots |

If a sentence explains why, it belongs in an ADR. Move it there.

## Slot templates

See `writing-templates.md`.
