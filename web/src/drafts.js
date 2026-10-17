// Bounded, device-local drafts. Stored data never submits itself.
const KEY = "glass.drafts.v1",
  MAX = 24,
  AGE = 30 * 86400000;
export function draftStore(onError) {
  let rows = {},
    timer;
  try {
    const value = JSON.parse(localStorage.getItem(KEY) || "{}");
    if (value && typeof value === "object" && !Array.isArray(value))
      rows = Object.fromEntries(
        Object.entries(value)
          .filter(
            ([, v]) =>
              v &&
              typeof v.text === "string" &&
              Number.isFinite(v.at) &&
              v.at > Date.now() - AGE,
          )
          .sort((a, b) => b[1].at - a[1].at)
          .slice(0, MAX)
          .map(([k, v]) => [k, { text: v.text.slice(0, 60000), at: v.at }]),
      );
  } catch {
    onError();
  }
  const persist = () => {
    clearTimeout(timer);
    rows = Object.fromEntries(
      Object.entries(rows)
        .filter(
          ([, v]) => v && typeof v.text === "string" && v.at > Date.now() - AGE,
        )
        .sort((a, b) => b[1].at - a[1].at)
        .slice(0, MAX),
    );
    try {
      localStorage.setItem(KEY, JSON.stringify(rows));
    } catch {
      onError();
    }
  };
  return {
    read(id) {
      return rows[id]?.text?.slice(0, 60000) || "";
    },
    write(id, value) {
      if (value) rows[id] = { text: value.slice(0, 60000), at: Date.now() };
      else delete rows[id];
      if (Object.keys(rows).length > MAX)
        rows = Object.fromEntries(
          Object.entries(rows)
            .sort((a, b) => b[1].at - a[1].at)
            .slice(0, MAX),
        );
      clearTimeout(timer);
      timer = setTimeout(persist, 300);
    },
    flush: persist,
  };
}
