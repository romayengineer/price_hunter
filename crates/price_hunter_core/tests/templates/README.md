# templates formatting

Templates use Askama (Jinja) syntax `{{ }}` / `{% %}`. Use a formatter that understands curly brackets — `prettier`/`tidy` break them.

## Tool: djLint (djhtml)

`djLint` is the successor to `djhtml`; both preserve Jinja/Askama tags.

```bash
pipx install djlint
# format all templates
djlint --reformat tests/templates/
# check only (CI)
djlint --check tests/templates/

# lighter alternative (formatter only)
pipx install djhtml
djhtml tests/templates/
```

Pre-commit (optional):

```yaml
- repo: https://github.com/Riverside-Healthcare/djLint
  rev: v1.35.4
  hooks:
    - id: djlint-reformat-django
```

## Patterns to avoid

Hand-written templates must keep delimiters on one line and spaced (`{{ var }}` not `{{var }}`):

open bracket not closed on same line
\{[^\}]*$

line starts with spaces then `}}`
^[ ]+\}\}

`{{` at end of line
\{\{[ ]*$

`}}` not preceded by space
[^ ]\}\}

`{{` not followed by space
\{\{[^ ]