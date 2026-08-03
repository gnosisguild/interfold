# ASD-STE100 writing rules

This reference contains the detailed rules for the `asd-ste100` skill. Read the sections that apply
to the current writing task.

## Contents

- [Purpose](#purpose)
- [Compliance boundary](#compliance-boundary)
- [Activation and scope](#activation-and-scope)
- [Order of authority](#order-of-authority)
- [Project resources](#project-resources)
- [Default behavior](#default-behavior)
- [Writing process](#writing-process)
- [Core writing rules](#core-writing-rules)
- [General grammar recommendations](#general-grammar-recommendations)
- [Requirements and specifications](#requirements-and-specifications)
- [Software and code documentation](#software-and-code-documentation)
- [Tables, lists, and headings](#tables-lists-and-headings)
- [Preservation rules for rewrites](#preservation-rules-for-rewrites)
- [STE-aligned fallback](#ste-aligned-fallback-when-the-dictionary-is-unavailable)
- [Final audit](#final-audit)
- [Response templates](#response-templates)
- [Maintenance](#maintenance)

## Purpose

Use this skill whenever you write, rewrite, summarize, translate into English, or review technical
prose.

Produce text that is easy to understand, difficult to misread, and suitable for an international
audience. Preserve the technical meaning before you simplify the language.

## Compliance boundary

ASD-STE100 has two necessary parts:

1. Writing rules.
2. A controlled dictionary that specifies approved words, meanings, parts of speech, and forms.

This skill contains an operational paraphrase of the writing rules. It does not reproduce the
controlled dictionary.

Use these compliance labels:

- **STE-verified**: Use only when you checked the text against the official Issue 9 dictionary,
  applicable project terminology, and all applicable writing rules.
- **STE-aligned**: Use when the official dictionary or complete project terminology is not
  available.

Never claim that text is ASD-STE100 compliant, certified, approved, or validated only because this
skill was applied.

ASD-STE100 and Simplified Technical English are owned marks and copyrighted material of ASD. This
independent skill is not the official standard and is not endorsed by ASD. Obtain the current
official copy from the ASD Simplified Technical English Maintenance Group website.

## Activation and scope

Apply this skill automatically to technical prose, including:

- Procedures and work instructions
- System, product, component, and software descriptions
- Safety instructions
- Requirements and specifications
- Installation, operation, maintenance, and troubleshooting text
- API, command-line, configuration, and code documentation
- Technical reports, release notes, help text, and support content
- Natural-language comments in code when the comments can be changed safely

Do not rewrite protected literals. Preserve these items exactly unless the user explicitly requests
a change:

- Source code, commands, queries, regular expressions, and equations
- Identifiers, symbols, constants, function names, class names, and API names
- File names, paths, URLs, addresses, keys, and configuration values
- Part numbers, serial numbers, model names, document identifiers, and cross-references
- UI labels, placard text, error messages, log output, and quoted text
- Legal, contractual, regulatory, or standards text that must remain verbatim

Apply STE rules to the prose around protected literals.

## Order of authority

Use this precedence when instructions conflict:

1. Safety, factual accuracy, and preservation of technical intent
2. Law, regulation, contract, certification basis, and official project directives
3. User instructions and approved project style rules
4. Approved project terminology
5. The official ASD-STE100 Issue 9 standard and dictionary, when available
6. This skill's STE-aligned fallback rules

Do not change a fact, limit, tolerance, hazard level, condition, sequence, requirement, or
responsibility to satisfy a language rule. Report the conflict instead.

## Project resources

Before substantial writing or review, look for these resources in the project:

- An official ASD-STE100 Issue 9 copy
- An approved-word list or checker configuration
- A terminology database, glossary, data dictionary, or nomenclature list
- A project style guide
- Safety-word definitions and hazard-classification rules
- Existing approved examples or translation memory

Possible file names include `ASD-STE100*`, `ste-terms.*`, `terminology.*`, `glossary.*`,
`nomenclature.*`, and `style-guide.*`.

Treat a technical noun [a project-approved name for a component, material, process, document,
system, or technical concept] as approved only when the project, industry, or subject field supports
it.

Treat a technical verb [a project-approved verb for a specialized technical action] as approved only
when the project, industry, or subject field supports it.

Do not infer that a general word is dictionary-approved only because it is common English.

## Default behavior

When the user asks for a rewrite, return only the rewritten text unless the user asks for analysis.

When the user asks for a review, return:

1. The compliance status: `STE-verified` or `STE-aligned`.
2. The revised text.
3. Only material unresolved items, such as an unknown term, ambiguous source meaning, or safety
   conflict.

When information is missing and invention could change the technical meaning, insert a precise
marker:

`[NEEDS TECHNICAL INPUT: state the missing fact or decision]`

Do not use a marker for a routine stylistic choice that you can resolve safely.

## Writing process

For each task:

1. Classify each passage as procedure, description, safety instruction, requirement, or protected
   literal.
2. Identify facts, conditions, sequence, limits, units, references, hazard information, and
   protected terms.
3. Select one approved term for each concept.
4. Rewrite the structure, not only individual words.
5. Apply the applicable rules below.
6. Count sentence words with the Section 8 rules.
7. Perform the final audit before you return the text.

## Core writing rules

The numbered items below paraphrase the 53 writing rules in ASD-STE100 Issue 9. When an official
copy is available, the official text controls.

### Section 1 — Words

**1.1** Use only words approved by the dictionary, approved technical nouns, and approved technical
verbs.

**1.2** Use an approved word only as its approved part of speech [its grammatical function, such as
noun, verb, or adjective].

**1.3** Use an approved word only with its approved meaning.

**1.4** Use only approved forms of verbs and adjectives.

**1.5** Use an unlisted word as a technical noun only when it fits an applicable technical-noun
category.

**1.6** Use a dictionary-unapproved word only when it is an approved technical noun or part of one.

**1.7** Do not use a technical noun as a verb.

**1.8** Use technical nouns accepted by the applicable company, industry, standard, or subject
field.

**1.9** When you select a technical noun, select a short term that is easy to understand.

**1.10** Do not use regional language, slang, or informal jargon as technical nouns.

**1.11** Use one technical noun for one item or concept. Do not alternate between synonyms.

**1.12** Use an unlisted verb as a technical verb only when it fits an applicable technical-verb
category.

**1.13** Do not use a technical verb as a noun.

**1.14** Use American English spelling unless an official directive requires a different spelling
system.

### Section 2 — Multi-word nouns

**2.1** Do not make a noun cluster [consecutive nouns that function as one name] with more than
three nouns.

**2.2** When an official technical noun is longer, write its full form first. Then define a clear
short name or use approved hyphenation.

Do not invent an abbreviation or short name when it could conflict with an existing term.

### Section 3 — Verbs

**3.1** Use only the verb forms permitted by the dictionary entry or approved technical terminology.

**3.2** Use simple verb constructions. Permitted forms are generally the infinitive, imperative,
simple present, simple past, simple future, and an approved past participle used as an adjective.

**3.3** Use a past participle as an adjective when it describes a condition. Do not use this
structure to hide the person or item that does an action.

**3.4** Do not use auxiliary verbs [helping verbs] to make complex verb constructions. Avoid perfect
and continuous tenses unless an official dictionary rule explicitly permits the construction.

**3.5** Use an `-ing` form only when it is an approved technical noun or a modifier in an approved
technical noun.

**3.6** Use the active voice. In descriptive text, use the passive voice only when the agent [the
person or item that does the action] is unknown.

**3.7** Use a verb to express an action. Do not hide the action in a noun.

Prefer:

`Adjust the valve.`

Do not write:

`Do an adjustment of the valve.`

### Section 4 — Sentences

**4.1** Write short, clear sentences.

**4.2** Do not omit necessary subjects, verbs, articles, or other words to shorten a sentence. Do
not use contractions such as `don't`, `can't`, or `isn't`.

**4.3** Use a vertical list when a sentence contains complex, parallel, or multiple items.

**4.4** Use clear connecting words or phrases when related sentences require an explicit logical
connection.

**4.5** Use an article (`a`, `an`, or `the`) or a demonstrative adjective (`this`, `that`, `these`,
or `those`) before a noun when standard English requires it.

### Section 5 — Procedural writing

A procedure tells the reader to do actions. Safety instructions in procedures follow the procedural
sentence limit.

**5.1** Use no more than 20 words in each procedural or safety sentence.

**5.2** Put only one instruction in each sentence. Combine actions only when they occur at the same
time or form one immediate action-result pair.

**5.3** Write instructions in the imperative [command] form. Start with the action verb when
practical.

Prefer:

`Close the valve.`

Do not write:

`You should close the valve.`

**5.4** When a condition or descriptive clause comes before a command, put a comma between the
clause and the command.

Example:

`When the pressure is stable, close the valve.`

**5.5** Use a note only for information. Do not put an instruction, required action, hazard control,
or missing procedure step in a note.

A reader must be able to complete the procedure correctly without reading the notes.

### Section 6 — Descriptive writing

A description gives information. It does not command the reader.

**6.1** Give information gradually, from general context to specific detail.

**6.2** Use repeated key words and clear key phrases to show the logical structure. Do not replace
key terms with stylistic synonyms.

**6.3** Use no more than 25 words in each descriptive sentence.

**6.4** Use paragraphs to group related information.

**6.5** Put only one topic in each paragraph.

**6.6** Use no more than six sentences in each paragraph.

Do not use the imperative form in descriptive text unless the passage is actually an instruction.

### Section 7 — Safety instructions

**7.1** Use the project-approved signal word for the risk level. Do not invent, downgrade, or
upgrade a hazard classification.

**7.2** Start a safety instruction with a clear command or a precise condition.

**7.3** State the hazard, risk, or possible result. Make the relationship between the unsafe
condition and the result explicit.

Keep safety controls in the work sequence. Do not move them into notes or general background text.

### Section 8 — Punctuation and word count

**8.1** Use standard English punctuation, but do not use a semicolon.

**8.2** Use hyphens to connect words that are directly related. Do not use a hyphen as a substitute
for a dash or an unclear sentence connection.

**8.3** Parentheses are permitted. Use them only for concise secondary information, identifiers,
references, alternatives, or clarification. Do not hide an essential action or hazard in
parentheses.

**8.4** In a vertical list, treat the colon before the list as a sentence boundary for word count.
Treat each list item as a separate sentence.

**8.5** Parenthetical text counts as one word in the containing sentence. The text inside the
parentheses is also a separate sentence and must meet its applicable limit.

**8.6** Count each of these as one word for sentence-length checks:

- A number
- A number with its unit of measurement
- An abbreviation
- An alphanumeric identifier
- A block of quoted text
- A title, heading, placard, or label
- A proper name of a person, group, organization, or geopolitical entity

**8.7** Count a correctly hyphenated term as one word.

For ordinary words, count space-separated words. Do not manipulate punctuation only to pass a word
limit.

### Section 9 — Writing practices

**9.1** When word replacement is not sufficient, rewrite the complete sentence construction.

**9.2** Use every approved word correctly in context.

**9.3** Do not create an unapproved phrasal verb [a verb plus a particle that creates a new
meaning]. Rewrite it with an approved single verb or an unambiguous construction.

**9.4** Use terminology, wording, capitalization, numbers, units, list structure, and references
consistently.

## General grammar recommendations

### GR-1 — `that`

Use `that` when it clearly separates a main clause from a subordinate clause. Always write
`make sure that`, not `make sure` followed directly by a clause.

### GR-2 — `with`

The word `with` can show association, shared action, or an instrument. Rewrite a sentence when
`with` could have more than one meaning.

Ambiguous:

`Attach the bracket with the clamp.`

Possible clear forms:

- `Use the clamp to attach the bracket.`
- `Attach the bracket that has the clamp.`
- `Attach the bracket and the clamp.`

Select only the form that matches the source meaning.

### GR-3 — Pronouns

Use a pronoun only when its antecedent [the noun that the pronoun refers to] is clear and close.
Repeat the technical noun when two or more references are possible.

Do not use a pronoun that refers forward to a noun that appears later in the sentence.

### GR-4 — `this`

Make the reference of `this` explicit. Prefer `this valve`, `this result`, or `this condition` to a
bare `this`.

### GR-5 — False friends

Avoid false friends [words that resemble a word in another language but have a different meaning].
Use the approved English term and verify uncertain international terminology.

### GR-6 — Latin abbreviations

Do not use Latin abbreviations such as `e.g.`, `i.e.`, `cf.`, or `etc.`. Write the meaning in
English, such as `for example` or `that is`.

### GR-7 — Inclusive language

Use inclusive, neutral, and non-discriminatory language. Do not infer a person's sex, gender,
ability, nationality, or other personal characteristic when it is not technically relevant.

### GR-8 — Possessive form

The apostrophe possessive is permitted, but it can be difficult for international readers. Use it
only when the relationship is clear and the construction is correct. When uncertain, use an `of`
phrase or another explicit construction.

## Requirements and specifications

For a mandatory requirement, use `must` unless the controlling project standard specifies another
keyword.

Use `can` only for capability or physical possibility. Do not use `can` to give permission unless
the project terminology explicitly permits that meaning.

Avoid `should`, `would`, `could`, and `might` unless the official dictionary and project rules
approve the intended meaning.

Write one requirement per sentence. State the responsible system, component, or person as the
subject.

Prefer:

`The controller must store 100 event records.`

Do not write:

`A capacity of 100 event records should be provided.`

## Software and code documentation

Preserve code symbols and exact interface text. Use approved project terms for software concepts.

For instructions, write one action per numbered step:

1. Open `config.yaml`.
2. Set `retry_count` to `3`.
3. Save the file.
4. Restart the service.

Do not replace an exact command, option, API field, status value, or error message with a synonym.

When a technical verb is necessary, such as `compile`, `deserialize`, `hash`, or `reindex`, treat it
as a project technical verb and use it consistently.

## Tables, lists, and headings

Use a vertical list when it reduces sentence complexity. Make every list item grammatically
parallel.

For a procedural list, start each action item with an imperative verb.

For a descriptive list, use all noun phrases or all complete sentences. Do not mix structures
without a technical reason.

Use short headings that name the topic. Do not put multiple topics in one heading.

## Preservation rules for rewrites

A rewrite must preserve:

- All technical facts and relationships
- Preconditions, branches, exceptions, and stop conditions
- Action sequence and responsible actor
- Requirement strength
- Safety signal words and hazard severity
- Numbers, signs, units, ranges, limits, tolerances, and precision
- Part numbers, identifiers, references, labels, and commands
- Defined terms and their capitalization
- The distinction between observed fact, assumption, estimate, and recommendation

Do not add a step because it seems useful. Do not delete a repeated statement when the repetition
has a safety or procedural function.

When the source has a material contradiction, do not silently choose one version. Preserve the
conflict and identify it as an unresolved technical issue.

## STE-aligned fallback when the dictionary is unavailable

When the official dictionary is not available:

- Use common, concrete words with one clear meaning.
- Prefer `use` to `utilize`.
- Prefer `before` to `prior to`.
- Prefer `after` to `subsequent to`.
- Prefer a direct verb to a noun phrase that hides an action.
- Avoid idioms, metaphors, humor, rhetorical language, and culture-specific references.
- Avoid unexplained abbreviations.
- Preserve legitimate domain terms as technical nouns or technical verbs.
- Flag a questionable general word instead of claiming that it is approved.

These substitutions are heuristics, not proof of dictionary compliance.

## Final audit

Before you return technical text, verify all applicable items:

1. The technical meaning and safety intent are unchanged.
2. Protected literals are exact.
3. Each concept has one consistent term.
4. General words are dictionary-approved, or the output is labeled STE-aligned.
5. Approved words use the correct meaning, part of speech, and form.
6. Technical nouns and technical verbs are legitimate project terms.
7. Noun clusters contain no more than three nouns, unless a long official name is introduced and
   clarified.
8. Verb constructions are simple and active.
9. Procedures use imperative verbs and one instruction per sentence.
10. Procedural and safety sentences contain no more than 20 words.
11. Descriptive sentences contain no more than 25 words.
12. Each descriptive paragraph has one topic and no more than six sentences.
13. Notes contain information only.
14. Safety text uses the correct signal word, command or condition, and consequence.
15. Pronouns, `this`, and `with` are unambiguous.
16. The text has no contractions, semicolons, unapproved Latin abbreviations, or avoidable phrasal
    verbs.
17. Lists are parallel and word counts follow the special counting rules.
18. The output does not make an unsupported compliance claim.

## Response templates

### Rewrite request

Return only the revised text.

### Review request

```text
Status: STE-aligned | STE-verified

Revised text:
[revised content]

Open items:
- [Only material terminology, ambiguity, safety, or source-content issues]
```

Omit `Open items` when there are no material issues.

### Compliance-check request

```text
Status: STE-aligned | STE-verified

Result: Pass | Needs revision | Cannot verify

Material findings:
- [rule or category]: [finding]

Revised text:
[content, when requested]
```

Do not produce a long style commentary unless the user requests it.

## Maintenance

The standard can change. Check the official ASD STEMG source before updating this skill or claiming
support for a later issue.

Official source: https://www.asd-ste100.org/
