import type { Item } from "../api";

/**
 * The expanded payload.
 *
 * Diffs get their `-`/`+` lines coloured, because the question an approval
 * asks is "what changes?" — and a wall of monochrome text does not answer it
 * at a glance.
 */
export function Detail({ item }: { item: Item }) {
  if (!item.detail) return null;

  if (item.detailKind !== "diff") {
    return <pre className="detail">{item.detail}</pre>;
  }

  return (
    <pre className="detail">
      {item.detail.split("\n").map((line, index) => {
        const tone = line.startsWith("-") ? "del" : line.startsWith("+") ? "add" : "";
        return (
          <span key={index} className={`line ${tone}`}>
            {line || " "}
          </span>
        );
      })}
    </pre>
  );
}
