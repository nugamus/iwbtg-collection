# The client: Import

Part of [the client](../client.md). Code: `import.rs`, and `package.rs` for opening the file (#36).

- **Opening an `.iwbz`.** `iwbtg-client <file>.iwbz` opens a package file like a package folder. `package::open` unpacks it with `iwbtg_format::iwbz::unpack`, which refuses anything a package may not hold and every zip bomb or path trick ([iwbz.md](../../format/iwbz.md)); the file already carries its engine pack, so nothing is read from `../_engines`.
- **The library.** An `.iwbz` the client opens is copied to `library/<id>.iwbz` in the data folder ([storage](storage.md); `%LOCALAPPDATA%\iwbtg-collection\library` on Windows), unless that copy is already the same file. A `--screenshot` check copies nothing. Game select (M6) will list the library; until then nothing reads it.
- **Drag and drop.** Dropping an `.iwbz` file or a package folder on the window opens it: the client checks that it loads, starts a new client on it (`current_exe <path>`, no other flags) and exits; a run in progress ends as on any exit, and its replay is kept. Something that is not a package, or a package with errors, only prints a warning and the game goes on. A message on screen is #648.
- **Double-click (file association).** Opening a file from the OS runs `iwbtg-client <file>`, which is the command-line path above. Registering `.iwbz` with the OS needs an installer, which comes with the release builds (M9): #647.
