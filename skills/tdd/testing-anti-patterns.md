# Testing Anti-Patterns

Load this reference when writing or changing tests, adding mocks, or tempted to add a test-only method to production code.

Tests verify real behavior, not mock behavior. A mock is a means of isolating the code under test, not the thing under test, so an assertion only the mock can satisfy proves nothing about the system. Running the cycle in `skills/tdd/SKILL.md` prevents most of what follows, because watching a test fail against real code is what exposes an assertion the mock alone satisfies.

## Testing Mock Behavior

```typescript
// Bad: asserting that the mock exists
test('renders sidebar', () => {
  render(<Page />);
  expect(screen.getByTestId('sidebar-mock')).toBeInTheDocument();
});
```

This passes when the mock is present and fails when it is not, which says nothing about whether the component works. Your human partner's correction sounds like "are we testing the behavior of a mock?".

```typescript
// Good: test the real component, or don't mock it
test('renders sidebar', () => {
  render(<Page />);
  expect(screen.getByRole('navigation')).toBeInTheDocument();
});
```

Before asserting on a mock element, work out whether the assertion covers real component behavior or mock existence. If it is mock existence, delete the assertion or unmock the component. If the component has to stay mocked for isolation, assert on the containing component's behavior instead of on the mock.

## Test-Only Methods in Production

```typescript
// Bad: destroy() is only ever called from tests
class Session {
  async destroy() {
    await this._workspaceManager?.destroyWorkspace(this.id);
  }
}

afterEach(() => session.destroy());
```

A production class carrying test-only code reads as production API, is dangerous if something calls it for real, and confuses object lifecycle with entity lifecycle.

```typescript
// Good: test utilities own test cleanup
export async function cleanupSession(session: Session) {
  const workspace = session.getWorkspaceInfo();
  if (workspace) {
    await workspaceManager.destroyWorkspace(workspace.id);
  }
}

afterEach(() => cleanupSession(session));
```

Before adding a method to a production class, ask whether tests are its only caller and whether the class owns the lifecycle of the resource the method touches. Either answer coming back wrong puts the method in test utilities, or on a different class.

## Mocking Without Understanding

```typescript
// Bad: the mock removes a side effect the test depends on
test('detects duplicate server', () => {
  vi.mock('ToolCatalog', () => ({
    discoverAndCacheTools: vi.fn().mockResolvedValue(undefined)
  }));

  await addServer(config);
  await addServer(config);  // should throw, but won't
});
```

The mocked method wrote the config that the second call needed in order to detect the duplicate. Mocking broadly to be safe removes the behavior under test, and the result passes for the wrong reason or fails inexplicably.

```typescript
// Good: mock at the level that is actually slow
test('detects duplicate server', () => {
  vi.mock('MCPServerManager');

  await addServer(config);
  await addServer(config);
});
```

Before mocking a method, establish what side effects the real one has and whether the test depends on any of them. If it does, mock further down at the slow or external operation, or use a double that preserves the behavior the test needs. If you cannot tell what the test depends on, run it against the real implementation first and observe, then add the minimum mocking at the right level. "I'll mock this to be safe" and "this might be slow, better mock it" are the sentences that precede this failure.

## Incomplete Mocks

```typescript
// Bad: only the fields this test reads
const mockResponse = {
  status: 'success',
  data: { userId: '123', name: 'Alice' }
};
// breaks later when code reads response.metadata.requestId
```

A partial mock encodes only the structure you happened to know about, so code depending on the omitted fields fails silently and the passing test is false confidence rather than evidence.

```typescript
// Good: mirror the real response
const mockResponse = {
  status: 'success',
  data: { userId: '123', name: 'Alice' },
  metadata: { requestId: 'req-789', timestamp: 1234567890 }
};
```

Mock the complete data structure as it exists in reality, not the subset the immediate test reads. Check the real response in docs or a captured example, include every field the system may consume downstream, and when uncertain include all documented fields.

## Tests as an Afterthought

"Implementation complete, ready for testing" is not a complete implementation. Testing is part of implementing, and the cycle in `skills/tdd/SKILL.md` (failing test, implement, refactor, then claim complete) is what keeps it there.

## When Mocks Get Too Complex

Mock setup longer than the test logic, mocking everything to get a pass, mocks missing methods the real components have, and tests that break whenever the mock changes all point the same way. Your human partner's question sounds like "do we need to be using a mock here?". Integration tests against real components are frequently simpler than the mocks they would replace.

Other signals worth stopping on: an assertion matching a `*-mock` test id, a method whose only callers live in test files, mock setup that is more than half the test, a test that fails when the mock is removed, and a mock whose necessity you cannot explain.

## Quick Reference

| Anti-pattern | Fix |
|--------------|-----|
| Assert on mock elements | Test the real component or unmock it |
| Test-only methods in production | Move to test utilities |
| Mock without understanding | Understand dependencies first, mock minimally |
| Incomplete mocks | Mirror the real response completely |
| Tests as an afterthought | Tests first |
| Over-complex mocks | Consider integration tests |
