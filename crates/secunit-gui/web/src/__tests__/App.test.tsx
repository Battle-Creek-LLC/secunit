import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { App } from "../App";

describe("App", () => {
  it("renders the product name", () => {
    render(<App />);
    expect(
      screen.getByRole("heading", { level: 1, name: "secunit" }),
    ).toBeInTheDocument();
  });

  it("shows the read-only viewer label with a version", () => {
    render(<App />);
    expect(screen.getByText(/read-only viewer/)).toBeInTheDocument();
  });
});
