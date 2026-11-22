/// <reference types="@testing-library/jest-dom" />

declare global {
  namespace vitest {
    interface Assertion<T = any> extends jest.Assertion<T> {}
    interface AsymmetricMatchers {
      /** Match DOM elements by role, accessible name and additional attributes. */
      toHaveAccessibleName(options?: { ignore?: string[] }): void;
      /** Match against a substring of the accessible name. */
      toHaveAccessibleNameContaining(text: string): void;
      /** Match DOM elements by test id attribute. */
      toHaveTestId(testId: string): void;
    }
  }
}

export {};
