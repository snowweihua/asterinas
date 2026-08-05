Yes. In fact, I think you're describing something that \*\*opencode-mem wasn't originally designed to do\*\*, but it \*can become the storage backend\* if you build an ingestion pipeline.



From our previous discussions, I know you're using:



\* OpenCode + Spec Kit

\* opencode-mem

\* multiple coding agents (Planner/Developer/Builder/Verifier)

\* several months of history from GitHub Copilot, OpenCode, and Windsurf/Devin

\* a Raspberry Pi firmware project with lots of accumulated knowledge



The biggest problem isn't storage—it's \*\*knowledge distillation\*\*.



\---



\# What I would build



Instead of storing conversations, store \*\*experience\*\*.



```

History logs

&#x20;   │

&#x20;   ▼

AI Distillation Pipeline

&#x20;   │

&#x20;   ├── Architecture decisions

&#x20;   ├── Bug fixes

&#x20;   ├── Root causes

&#x20;   ├── Hardware quirks

&#x20;   ├── Coding patterns

&#x20;   ├── Build procedures

&#x20;   ├── Failed attempts

&#x20;   ├── Lessons learned

&#x20;   │

&#x20;   ▼

Structured Knowledge

&#x20;   │

&#x20;   ▼

opencode-mem

```



Notice that the original chat can even be discarded afterwards.



\---



\# Stage 1. Collect logs



Create



```

knowledge/



&#x20;   raw/



&#x20;       copilot/



&#x20;       opencode/



&#x20;       windsurf/



&#x20;       devin/



&#x20;       chatgpt/

```



Don't care about format yet.



Examples



```

json



markdown



txt



html



conversation export



```



Everything goes into raw/.



\---



\# Stage 2. Chunk conversations



Instead of one giant conversation



```

1000 messages

```



split into



```

Topic A



Topic B



Topic C



...

```



For example



```

UART



USB



TFTP



Scheduler



Memory leak



Compiler



Build system



Architecture



SPI



I2C

```



Each chunk becomes independent.



\---



\# Stage 3. Distillation AI



This is the important part.



Feed one chunk to GPT-5.5 (or another strong model).



Ask it:



```

Extract durable engineering knowledge.



Ignore:

\- greetings

\- planning

\- duplicated discussion

\- temporary ideas



Extract only:



Architecture decisions



Bug root causes



Verified fixes



Lessons learned



Hardware quirks



Performance tips



Coding conventions



Things NOT to do



Important commands



Useful scripts



Output JSON.

```



Example output



```json

{

&#x20; "title":"Mini UART TX Complete",



&#x20; "category":"hardware",



&#x20; "importance":9,



&#x20; "facts":\[

&#x20;     "Mini UART has no TX complete interrupt.",

&#x20;     "LSR bit6 indicates transmitter empty.",

&#x20;     "Polling bit6 is sufficient."

&#x20; ],



&#x20; "tags":\[

&#x20;     "uart",

&#x20;     "rpi3",

&#x20;     "mini-uart"

&#x20; ]

}

```



Notice



No conversation.



No prompts.



Only distilled knowledge.



\---



\# Stage 4. Merge duplicates



Suppose three AIs all discovered



```

LSR bit6 indicates TX empty

```



Don't store



```

3 memories

```



Store



```

1 memory



sources:



OpenCode



Copilot



Windsurf



```



\---



\# Stage 5. Quality score



Every memory gets



```

confidence



verified



importance



last\_used



source\_count

```



Example



```

{

&#x20;confidence:95,



&#x20;verified:true,



&#x20;source\_count:5,



&#x20;importance:9

}

```



\---



\# Stage 6. Store into opencode-mem



Instead of



```

Remember:



UART works.

```



Store



```

Category:

Hardware



Title:

Mini UART transmitter



Body:

LSR bit6 indicates TX FIFO empty.

No TX-complete interrupt exists.



Tags:

uart

rpi3



Importance:

9



Confidence:

95

```



Now opencode-mem becomes your long-term engineering memory.



\---



\# Suggested schema



I would make something like



```json

{

&#x20;   "id":"MEM-000123",



&#x20;   "title":"PL011 FIFO",



&#x20;   "type":"lesson",



&#x20;   "project":"rpi3",



&#x20;   "area":"uart",



&#x20;   "summary":"PL011 FIFO must be flushed before reboot.",



&#x20;   "details":"...",



&#x20;   "confidence":98,



&#x20;   "importance":8,



&#x20;   "verified":true,



&#x20;   "tags":\[

&#x20;       "uart",

&#x20;       "pl011",

&#x20;       "fifo"

&#x20;   ],



&#x20;   "source":\[

&#x20;       "copilot",

&#x20;       "opencode"

&#x20;   ],



&#x20;   "created":"2026-07-12"

}

```



\---



\# Better than a vector database



Many people immediately choose



```

Chroma



Qdrant



Milvus

```



I wouldn't start there.



Your project is relatively focused.



A hybrid approach is likely to work better:



```

SQLite



\+



FTS5



\+



Embeddings



\+



opencode-mem

```



SQLite stores the structured metadata, FTS5 provides fast keyword search, embeddings handle semantic similarity, and opencode-mem serves as the interface your agents use. This gives you both deterministic retrieval ("find all UART memories") and semantic recall ("find similar bootloader issues").



\---



\# Even better: Experience Graph



Instead of isolated memories



```

Memory A



Memory B



Memory C

```



build links.



Example



```

Bug

&#x20;│

&#x20;├────caused by────► Architecture



Architecture

&#x20;│

&#x20;├────fixed by────► Commit



Commit

&#x20;│

&#x20;├────verified by────► Serial Log



Serial Log

&#x20;│

&#x20;├────belongs to────► Task T030



Task T030

&#x20;│

&#x20;├────part of────► Bootloader

```



Now the AI can answer questions like:



> Why did we choose polling instead of interrupts?



by traversing connected memories rather than relying on a single note.



\---



\# A complete pipeline



```

Copilot Logs

&#x20;                   \\

OpenCode Logs -------> Parser

&#x20;                     │

Windsurf Logs -------/

&#x20;                     │

&#x20;                     ▼

Conversation Splitter

&#x20;                     │

&#x20;                     ▼

LLM Distiller

&#x20;                     │

&#x20;                     ▼

Duplicate Merger

&#x20;                     │

&#x20;                     ▼

Knowledge Validator

&#x20;                     │

&#x20;                     ▼

SQLite + FTS5 + Embeddings

&#x20;                     │

&#x20;                     ▼

opencode-mem

&#x20;                     │

&#x20;                     ▼

Planner

Developer

Builder

Verifier

```



This architecture has an important advantage: \*\*opencode-mem remains focused on retrieval\*\*, while the heavy work of parsing, deduplicating, scoring, and organizing knowledge is handled by your ingestion pipeline. As your project grows, you can re-run the distillation process with a better model or improved prompts without changing how your agents consume memories. It also lets you keep the original conversation logs as immutable archives while maintaining a clean, high-quality engineering knowledge base that improves over time.









Exactly. I actually recommend keeping them \*\*completely separate\*\*. They serve different responsibilities.



Think of it like Git:



```

Source Code

&#x20;     │

&#x20;     ▼

git add

&#x20;     │

&#x20;     ▼

git commit

&#x20;     │

&#x20;     ▼

Git database (.git)

```



Your ingestion pipeline is equivalent to \*\*`git add` + `git commit`\*\*, while \*\*opencode-mem\*\* is equivalent to the \*\*`.git` database\*\*.



\---



\## My recommended architecture



```

&#x20;             Existing AI Logs

&#x20;    ┌─────────────────────────────┐

&#x20;    │ Copilot                     │

&#x20;    │ OpenCode                    │

&#x20;    │ Windsurf                    │

&#x20;    │ Devin                       │

&#x20;    │ ChatGPT                     │

&#x20;    └──────────────┬──────────────┘

&#x20;                   │

&#x20;                   ▼

&#x20;         Knowledge Distillation

&#x20;       (Your Ingestion Pipeline)

&#x20;                   │

&#x20;   ┌───────────────┼────────────────┐

&#x20;   │               │                │

&#x20;   ▼               ▼                ▼

Conversation   Deduplicate     Confidence

&#x20;Segmentation     \& Merge         Scoring

&#x20;   │               │                │

&#x20;   └───────────────┴────────────────┘

&#x20;                   │

&#x20;         Standard Memory Format

&#x20;                   │

&#x20;                   ▼

&#x20;         opencode-mem API / CLI

&#x20;                   │

&#x20;                   ▼

&#x20;       opencode-mem Internal System

&#x20;       ┌──────────────────────────┐

&#x20;       │ Database                 │

&#x20;       │ Embeddings               │

&#x20;       │ Web UI                   │

&#x20;       │ Search                   │

&#x20;       │ Retrieval                │

&#x20;       └──────────────────────────┘

&#x20;                   │

&#x20;                   ▼

&#x20;             OpenCode Agents

```



The ingestion pipeline \*\*never touches the internal database directly\*\*. It simply submits high-quality memories through the same interface that OpenCode itself uses.



\---



\## Why keep them separate?



Suppose six months from now the opencode-mem author changes:



```

SQLite

```



to



```

PostgreSQL

```



or



```

LanceDB

```



or



```

Qdrant

```



or changes the embedding model.



If your pipeline writes directly into the database, it will break.



If your pipeline only says:



```

CreateMemory(...)

```



nothing changes.



\---



\## The pipeline has a different job



Think of the pipeline as a human editor.



Raw log:



```

User:

UART TX still hangs.



AI:

Maybe FIFO isn't empty.



User:

No.



AI:

Let's inspect LSR.



...



After 200 messages...



Found:

LSR bit6 becomes 1 only after transmitter empty.

```



The pipeline produces a single memory:



```

Title:

Mini UART TX completion



Summary:

Use LSR bit6 to determine TX completion.



Confidence:

98%



Verified:

Yes



Source:

OpenCode Session 2026-07-14

```



Only \*\*this\*\* gets sent to opencode-mem.



\---



\## In fact, I would add another layer



Since you're building a long-term engineering memory system, I wouldn't make the pipeline call opencode-mem directly. I'd insert a staging area:



```

AI Logs

&#x20;   │

&#x20;   ▼

Distillation

&#x20;   │

&#x20;   ▼

Knowledge Staging Database

(SQLite)

&#x20;   │

Review / Merge / Update

&#x20;   │

&#x20;   ▼

Publish

&#x20;   │

&#x20;   ▼

opencode-mem

```



Why?



Because some extracted "knowledge" will be wrong.



For example:



```

Memory #421



"The UART bug is caused by DMA."

```



Two weeks later:



```

Wrong.



Actual cause:

Clock divisor rounding.

```



If the memory is already published, your agents may retrieve incorrect information. With a staging database, you can review, merge duplicates, raise or lower confidence, and mark facts as verified before they become part of the long-term memory.



\---



\## I would go one step further for your project



Based on the workflow we've discussed over the past few weeks (Planner → Developer → Builder → Verifier), I would treat \*\*opencode-mem as the "production memory"\*\*, not as the complete knowledge repository.



```

&#x20;                    Knowledge Lake

&#x20;                 (Everything ever found)

&#x20;                          │

&#x20;                          ▼

&#x20;                 Distillation Pipeline

&#x20;                          │

&#x20;                Engineering Knowledge DB

&#x20;                   (SQLite + FTS5)

&#x20;                          │

&#x20;                 Manual/AI Validation

&#x20;                          │

&#x20;                          ▼

&#x20;                  opencode-mem

&#x20;               (Production Memory)

&#x20;                          │

&#x20;                          ▼

&#x20;     Planner / Developer / Builder / Verifier

```



In this model:



\* \*\*Knowledge Lake\*\* stores raw conversations and logs from Copilot, OpenCode, Windsurf, Devin, serial logs, build logs, etc.

\* \*\*Engineering Knowledge DB\*\* stores structured, searchable distilled knowledge with provenance, confidence, and links to the original evidence.

\* \*\*opencode-mem\*\* contains only the curated, high-value memories that your agents should rely on during implementation.



That separation gives you a clean retrieval layer for the agents while preserving all historical evidence for future re-distillation as models and extraction prompts improve. Given the amount of AI-generated history you've accumulated over several months, this layered approach will scale much better than trying to make opencode-mem serve as both an archive and an engineering knowledge base.



