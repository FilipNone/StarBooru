//! What the empty search box shows: someone typing searches into it, each with
//! a line on what it does, a walk through the cheat sheet (`help.rs`).

use dioxus::prelude::*;

/// The tag the examples search for.
const TAG: &str = "ishtar_(fate)";

/// From the simplest search to all of it at once, as the cheat sheet goes.
fn examples() -> [(&'static str, String); 18] {
    [
        ("ishtar_(fate)", t!("demo-tag")),
        ("ishtar_(fate) smile", t!("demo-tag-all")),
        ("ishtar_(fate) -rin_tohsaka", t!("demo-tag-not")),
        ("{ishtar_(fate) red_eyes}", t!("demo-figure")),
        ("ishtar_(fate) rating:g", t!("demo-rating")),
        ("ishtar_(fate) rating:g rating:s", t!("demo-rating-any")),
        ("ishtar_(fate) -rating:e", t!("demo-rating-not")),
        ("ishtar_(fate) type:video", t!("demo-type")),
        ("ishtar_(fate) date:2024-03", t!("demo-date")),
        ("ishtar_(fate) date:2023..2024", t!("demo-date-range")),
        ("ishtar_(fate) date:<7d", t!("demo-date-age")),
        ("ishtar_(fate) width:>1920", t!("demo-size")),
        ("ishtar_(fate) ratio:16:9", t!("demo-ratio")),
        ("ishtar_(fate) color:red", t!("demo-color")),
        ("ishtar_(fate) text:\"gold\"", t!("demo-text")),
        ("ishtar_(fate) order:score", t!("demo-order-score")),
        ("ishtar_(fate) order:random", t!("demo-order-random")),
        (
            "ishtar_(fate) -rin_tohsaka rating:g order:score",
            t!("demo-combined"),
        ),
    ]
}

/// Writes straight into the page, so a keystroke costs no render. It rests while
/// the box has focus or nobody can see it, ends when the box gets a search, and
/// doesn't start under reduced motion or the battery saver: the input's own
/// placeholder shows then.
const TYPING: &str = r##"
window.__sbTyping ??= {
  run: 0,
  calm() {
    return matchMedia("(prefers-reduced-motion: reduce)").matches
      || !!document.querySelector('.app[data-saver="on"]');
  },
  async play(tag, examples) {
    const run = ++this.run;
    const demo = document.querySelector(".search-demo");
    const box = demo?.closest(".search");
    if (!box) return;
    const line = demo.querySelector(".demo-line");
    const base = demo.querySelector(".demo-base");
    const term = demo.querySelector(".demo-term");
    const note = demo.querySelector(".demo-note");
    const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
    const alive = () => run === this.run && demo.isConnected && !this.calm();
    // Waits out the time, and then for as long as it should rest.
    const beat = async (ms) => {
      await sleep(ms);
      while (alive() && (document.hidden || box.matches(":focus-within"))) await sleep(250);
      return alive();
    };
    // The tag itself stays quiet: what the example teaches is what's coloured.
    const show = (text) => {
      const quiet = text.startsWith(tag) ? tag.length : Math.min(text.length, tag.length);
      const plain = tag.startsWith(text) || text.startsWith(tag);
      base.textContent = plain ? text.slice(0, quiet) : "";
      term.textContent = plain ? text.slice(quiet) : text;
      line.scrollLeft = line.scrollWidth;
    };
    const play = async () => {
      if (!(await beat(700))) return;
      let typed = "";
      for (let i = 0; ; i = (i + 1) % examples.length) {
        const [query, meaning] = examples[i];
        note.classList.remove("shown");
        let same = 0;
        while (same < typed.length && typed[same] === query[same]) same++;
        while (typed.length > same) {
          typed = typed.slice(0, -1);
          show(typed);
          if (!(await beat(24))) return;
        }
        while (typed.length < query.length) {
          typed = query.slice(0, typed.length + 1);
          show(typed);
          const thought = /[\s:]/.test(typed.at(-1)) ? 150 : 0;
          if (!(await beat(40 + Math.random() * 70 + thought))) return;
        }
        note.textContent = meaning;
        note.classList.add("shown");
        if (!(await beat(2600))) return;
      }
    };
    if (this.calm()) return;
    box.dataset.typing = "";
    await play();
    // A newer run has the box now if the demo came back.
    if (run === this.run) delete box.dataset.typing;
  },
};
"##;

#[component]
pub fn SearchDemo() -> Element {
    use_effect(|| {
        let examples = serde_json::to_string(&examples()).unwrap_or_else(|_| "[]".into());
        document::eval(&format!(
            "{TYPING}\nwindow.__sbTyping.play({TAG:?}, {examples});"
        ));
    });
    rsx! {
        div { class: "search-demo", aria_hidden: "true",
            span { class: "demo-line",
                span { class: "demo-base" }
                span { class: "demo-term" }
                span { class: "demo-caret" }
            }
            span { class: "demo-note" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TAG, TYPING, examples};
    use starbooru_core::parse_query;

    #[test]
    fn every_example_is_a_search_that_works() {
        for (query, _) in examples() {
            assert!(parse_query(query).is_ok(), "{query}");
            assert!(query.contains(TAG), "{query}");
        }
    }

    #[test]
    fn no_two_examples_are_the_same() {
        let queries = examples().map(|(query, _)| query);
        for (i, query) in queries.iter().enumerate() {
            assert!(!queries[..i].contains(query), "{query}");
        }
    }

    #[test]
    fn the_lines_stay_short_enough_for_a_phone() {
        for (query, meaning) in examples() {
            assert!(meaning.chars().count() <= 40, "{query}: {meaning}");
        }
    }

    #[test]
    fn typing_keeps_to_calm_motion_and_stops_for_the_user() {
        assert!(TYPING.contains("prefers-reduced-motion"));
        assert!(TYPING.contains(r#"data-saver="on""#));
        assert!(TYPING.contains(":focus-within"));
        assert!(TYPING.contains("document.hidden"));
        assert!(TYPING.contains("demo.isConnected"));
    }

    #[test]
    fn the_coloured_term_stays_readable_on_the_glass() {
        let css = include_str!("../../assets/main.css");
        assert!(css.contains(".demo-term { color: var(--on-primary-container);"));
    }

    #[test]
    fn the_real_placeholder_only_gives_way_while_typing() {
        let css = include_str!("../../assets/main.css");
        assert!(css.contains(".search[data-typing]:not(:focus-within) input::placeholder"));
        assert!(css.contains(".search:not([data-typing]) .search-demo"));
    }
}
