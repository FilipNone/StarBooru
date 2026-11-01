# Contributing to StarBooru

Bug reports, ideas and pull requests are welcome. For anything big, open an issue first so nobody builds something that won't fit.

## Reporting a bug

Use the [bug report form](https://github.com/FilipNone/StarBooru/issues/new?template=bug_report.yml). Say what you did, what you expected and what happened, and on which client and version.

Your collection is private, so don't attach personal pictures. If a bug needs a file to reproduce, make a small one that shows it.

Security problems go through [SECURITY.md](SECURITY.md), not public issues.

## Making a change

1. Read [AGENTS.md](AGENTS.md). Its rules apply to every contributor, human or not. The main one: non-test code never panics.
2. Set up the toolchain with [docs/development.md](docs/development.md).
3. Add tests with your change. Parsers and validators get unit tests in `crates/core`, and server behaviour gets integration tests.
4. For a UI change, check it in the light and dark themes at desktop and phone (390 px) width, using generated or public-domain test images.
5. Run the checks in [AGENTS.md › Before you finish a task](AGENTS.md#4-before-you-finish-a-task).
6. Update `docs/` if behaviour changed, and `docs/api.md` for any change to the API.

Open the pull request against `main`. Keep it to one change, and write commit subjects like the existing ones (`fix: ...`, `feat: ...`).

## Translations

- **The interface:** each language is a Fluent catalogue in `apps/app/locales/`. Copy `en.ftl` and translate its messages. A test checks that every catalogue has every message with the same variables.
- **The README:** there are five copies: `README.md`, and `README.it.md`, `README.zh-CN.md`, `README.ja.md` and `README.ru.md` in `docs/readme/`. A change to one belongs in all of them, and corrections from native speakers are very welcome.

## License

By contributing, you agree that your work is released under the project's license, [AGPL-3.0-or-later](LICENSE). Third-party assets must be permissively licensed and get a row in [ATTRIBUTION.md](ATTRIBUTION.md).
