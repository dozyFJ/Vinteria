# Winteria DAW — правила для ИИ-помощников (ChatGPT/Codex и Claude)

## ⚠️ ОДНА ВЕТКА

**Вся работа над мобильной версией идёт ТОЛЬКО в одной ветке:**

> **репозиторий `dozyFJ/Vinteria`, ветка `winteria`**

- Новых веток не создавать. `winteria-chatgpt`, `winteria-mobile-pulse`, `android-*` и прочие устарели и
  уже влиты в `winteria`, туда больше не коммитить.
- Перед началом работы: `git fetch origin && git checkout winteria && git pull --rebase origin winteria`.
- После каждого законченного шага: `git push origin winteria`. Не копить изменения в облачной папке: второй
  помощник видит только то, что запушено.
- Если push отклонён (второй помощник успел запушить), сделай `git pull --rebase origin winteria`, реши
  конфликты, проверь сборку и пушь снова. Никогда не делай `push --force`.
- `ffight763-glitch/Vinteria-git3` — только зеркало для бесплатной сборки APK в GitHub Actions.
  Claude сам синхронизирует его с `dozyFJ/Vinteria:winteria`. Работать там не нужно.

## Кто что делает и как выходят обновления (с 1.0.36)

- **Ключ подписи** лежит в секретах GitHub репозитория `ffight763-glitch/Vinteria-git3`
  (`ANDROID_KEYSTORE_B64`, `ANDROID_KEYSTORE_PASSWORD`), сертификат SHA-256 `1ace6d87…7334e`
  (`RELEASE_CERT_SHA256` в `.github/workflows/android.yml`).
- **Выпуск обновления = push в `winteria`.** Claude синхронизирует зеркало, CI в Vinteria-git3 собирает APK, проверяет
  подпись и публикует OTA в ветку `ota` того же репозитория (`latest.json` + `releases/Winteria-<версия>.apk`).
  Приложение читает именно её (`FEED` в `src/android_update.rs`).
- **ChatGPT:** в `dozyFJ/pulse-updates` больше не публикуй, своим старым ключом не подписывай. Просто пушь в `winteria`
  (версия = следующая после `ota/latest.json`), остальное сделает CI.
- **Claude** делает фичи и исправления в той же ветке и следит за CI.

## Правила

1. **Версия:** следующая после последней опубликованной (`latest.json` в `pulse-updates/winteria`). Номер ставь в
   `Cargo.toml` в том же коммите, из которого собираешь.
2. **Журнал:** в конец `CLAUDE_HANDOFF.md` добавь раздел `### Android 1.0.X`: что сделано, что не проверено
   на телефоне.
3. **Секреты не коммитить:** ни `*.keystore`/`*.jks`, ни паролей, ни токенов.
4. **Перед выпуском APK** `tools/build-android.sh --release` проверяет C++ символы (иначе вылет на старте с
   `__cxa_pure_virtual`).
5. **Чужие сэмплы и паки** в репозиторий и APK не кладём.
6. **Дизайн:** на ПК стиль AOS, на телефоне Pulse. Планшеты от 7" и раскладушки в раскрытом виде получают ПК-интерфейс
   (план в `ROADMAP.md`).
