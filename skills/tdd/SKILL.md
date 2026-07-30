---
name: tdd
description: Use when implementing any feature or bugfix, before writing implementation code
---

# Test-Driven Development (TDD)

Write the test first, watch it fail, then write the minimal code that makes it pass.

The test is written and seen failing before the implementation exists. A test never seen red proves nothing: it may exercise the wrong path, assert on the implementation instead of the behavior, or pass for a reason unrelated to the feature, and you have no way to tell which. Watching it fail is the only evidence that it tests something.

## When to Use

New features, bug fixes, refactoring, and behavior changes all go through the cycle. Throwaway prototypes, generated code, and configuration files are the standing exceptions, and taking one is a decision to raise with your human partner rather than make alone.

If production code was written before its test, delete it and implement fresh from the tests. Keeping it as reference and adapting it while writing the tests is testing after with extra steps. Deleting hours of work feels wasteful, but that time is spent either way — what remains is a choice between code you can trust and code you cannot.

## Red-Green-Refactor

### RED — Write Failing Test

Write one minimal test showing what should happen: one behavior, a name describing that behavior, real code rather than mocks unless mocking is unavoidable.

Good:

```typescript
test('retries failed operations 3 times', async () => {
  let attempts = 0;
  const operation = () => {
    attempts++;
    if (attempts < 3) throw new Error('fail');
    return 'success';
  };

  const result = await retryOperation(operation);

  expect(result).toBe('success');
  expect(attempts).toBe(3);
});
```

Bad — vague name, and it asserts on the mock rather than on the code under test:

```typescript
test('retry works', async () => {
  const mock = jest.fn()
    .mockRejectedValueOnce(new Error())
    .mockRejectedValueOnce(new Error())
    .mockResolvedValueOnce('success');
  await retryOperation(mock);
  expect(mock).toHaveBeenCalledTimes(3);
});
```

### Verify RED — Watch It Fail

```bash
npm test path/to/test.test.ts
```

Confirm the test fails rather than errors, that the failure message is the one you expected, and that it fails because the feature is missing rather than because of a typo.

If it passes, it is testing behavior that already exists — fix the test. If it errors, fix the error and re-run until it fails correctly.

### GREEN — Minimal Code

Write the simplest code that passes the test. No extra features, no refactoring of neighboring code, no improvements beyond what the test demands.

Good — just enough to pass:

```typescript
async function retryOperation<T>(fn: () => Promise<T>): Promise<T> {
  for (let i = 0; i < 3; i++) {
    try {
      return await fn();
    } catch (e) {
      if (i === 2) throw e;
    }
  }
  throw new Error('unreachable');
}
```

Bad — options nothing asked for:

```typescript
async function retryOperation<T>(
  fn: () => Promise<T>,
  options?: {
    maxRetries?: number;
    backoff?: 'linear' | 'exponential';
    onRetry?: (attempt: number) => void;
  }
): Promise<T> {
  // YAGNI
}
```

### Verify GREEN — Watch It Pass

```bash
npm test path/to/test.test.ts
```

Confirm the test passes, the other tests still pass, and the output is pristine — no errors, no warnings. If the new test fails, fix the code, not the test. If other tests fail, fix them now.

### REFACTOR — Clean Up

Only after green: remove duplication, improve names, extract helpers. Tests stay green and behavior stays unchanged.

Then write the next failing test for the next behavior.

## Good Tests

| Quality | Good | Bad |
|---------|------|-----|
| **Minimal** | One thing. "and" in name? Split it. | `test('validates email and domain and whitespace')` |
| **Clear** | Name describes behavior | `test('test1')` |
| **Shows intent** | Demonstrates desired API | Obscures what code should do |

Tests written before the code force edge cases into the open while the design is still movable; tests written after only confirm the cases you happened to remember, and they inherit the implementation's blind spots.

## When Stuck

| Problem | Solution |
|---------|----------|
| Don't know how to test | Write wished-for API. Write assertion first. Ask your human partner. |
| Test too complicated | Design too complicated. Simplify interface. |
| Must mock everything | Code too coupled. Use dependency injection. |
| Test setup huge | Extract helpers. Still complex? Simplify design. |

## Debugging Integration

Found a bug? Write a failing test reproducing it, then run the cycle. The test proves the fix and prevents the regression, which is why bug fixes are not exempt from the ordering above.

## Testing Anti-Patterns

When adding mocks or test utilities, read @testing-anti-patterns.md to avoid common pitfalls:
- Testing mock behavior instead of real behavior
- Adding test-only methods to production classes
- Mocking without understanding dependencies
