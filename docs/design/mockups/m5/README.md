# Design pass 2 (m5): play modes, app shell, settings, tools and redesigns

**Proposal, not approved.** Drawn for #47 and the mode/shell issue linked from it; nothing here changes docs/product.md until the user approves. Live canvas: https://claude.ai/artifact/LvECtgqWCzfeNgtn1SKcFw. The approved design is still [m4](../m4/README.md); m5 reuses its language and its shared parts.

The source files are in this folder: `canvas.json` places the artboards and the notes (the rationale, costs and open questions are the sticky notes on the canvas), each `.dc.html` is one artboard. Most numbered artboards are thin wrappers that set props on a shared component (`MainMenu`, `Select`, `Shelf`, `Settings`, ...). Render them locally with `python -I docs/design/mockups/render.py m5` (see [m4's README](../m4/README.md#rendering-locally)).

## What is proposed

- **A. Two play modes.** Time Attack (today's Ranked: stripped, timed, leaderboards; cherry #ff3d55) and Classic (today's Play original: the full game with its own title, saves and difficulty select; violet #8f7bff). Direction A (recommended): the mode is chosen on the main menu and switched from the toolbar; Time Attack keeps m4's game select, Classic gets a shelf with save slots (answers #851), a slim session panel in place of the HUD, a Back-to-the-shelf pause item and a session summary instead of results. Direction B: one select with mode tabs.
- **B. App shell.** Intro, first-launch notice (#73), first-run setup, main menu with the fruit as centrepiece (logo slot for #201), the toolbar, exit and update dialogs, the web start and loading screens.
- **C. Play, settings and tools.** Settings overlay (#53), key bindings (#52), offset calibration (#464), practice picker and HUD (#39), Details tab (#498), collections (#105), replays, catalog (#50), profile (#60), sign-in (#158, #159).
- **D. Redesigns.** Script error (#953), notifications, game select gaps, auto-retry (#773), mod select (#554), exit flow (#1553), drag and drop, skip hint, pause on a pad (#624), port status (#62).
- **E. Cross-cutting.** Steam Deck (1280x800), Japanese, colourblind check, UI sounds. **F.** Race screens, rough.

## Shared parts changed from m4

`ProfileCard` gained a `mode` prop (Classic numbers and violet accent), `PauseBox` an `items` prop, `RunInfo` the `frozen`, `retry` and `status` marks. The m4 copies are unchanged.

## Artboards

| File | Artboard |
|---|---|
| `Main.dc.html` | A1 Main menu: Play opens the two modes (direction A) |
| `A2_MenuClassic.dc.html` | A2 Main menu: Classic focused, the accent turns violet |
| `A3_SelectTimeAttack.dc.html` | A3 Time Attack: the m4 game select under the toolbar (A) |
| `A4_Shelf.dc.html` | A4 Classic: the shelf, Continue or New game (A, #851) |
| `A5_ShelfNewGame.dc.html` | A5 Classic: New game over a slot with a save (#851) |
| `A6_LoadingClassic.dc.html` | A6 Loading card in Classic |
| `A7_ClassicTitle.dc.html` | A7 Classic run: the game's own title, session panel |
| `A8_ClassicPause.dc.html` | A8 Classic pause: Back to the shelf |
| `A9_Session.dc.html` | A9 Session summary on leaving Classic |
| `A10_SessionFinished.dc.html` | A10 Session summary after the ending |
| `A11_FallbackTitle.dc.html` | A11 Client-drawn title and menus for ports without their own (#100) |
| `A12_CardModes.dc.html` | A12 Player card per mode |
| `A13_BClassic.dc.html` | A13 Direction B: one select with mode tabs, Classic |
| `A14_BTimeAttack.dc.html` | A14 Direction B: one select with mode tabs, Time Attack |
| `B1_Intro.dc.html` | B1 Intro: the fruit lands on the spikes (1.8 s, any key skips) |
| `B2_Notice.dc.html` | B2 First launch: fan project, not affiliated, credits (#73) |
| `B3_SetupLanguage.dc.html` | B3 First run 1/3: language (#196) |
| `B4_SetupControls.dc.html` | B4 First run 2/3: controls check |
| `B5_SetupGames.dc.html` | B5 First run 3/3: where games come from (#51) |
| `B6_MenuInitial.dc.html` | B6 Main menu: initial (press any key) |
| `B7_MenuTop.dc.html` | B7 Main menu: top level |
| `B8_Toolbar.dc.html` | B8 Toolbar states |
| `B9_Exit.dc.html` | B9 Exit confirmation (only when something is running) |
| `B10_Update.dc.html` | B10 Update available (#115) |
| `B11_WebStart.dc.html` | B11 Web: click to start (audio unlock) |
| `B12_WebLoading.dc.html` | B12 Web: loading the package |
| `B13_WebError.dc.html` | B13 Web: the package did not load |
| `C1_Settings.dc.html` | C1 Settings: General and Graphics (#53) |
| `C2_SettingsAudio.dc.html` | C2 Settings: Audio, Input, Gameplay and HUD |
| `C3_SettingsOnline.dc.html` | C3 Settings: Online, account and data (#1491) |
| `C4_SettingsSearch.dc.html` | C4 Settings: search |
| `C5_Bindings.dc.html` | C5 Key bindings: listening, and a conflict (#52) |
| `C6_BindingsGame.dc.html` | C6 Key bindings: per game, extra actions (#52) |
| `C7_Calibrate.dc.html` | C7 Audio offset calibration (#464) |
| `C8_ExtraActions.dc.html` | C8 Before play: the game uses extra actions (#52) |
| `C9_PracticePicker.dc.html` | C9 Practice: start from a stage, save or room (#39) |
| `C10_PracticeHud.dc.html` | C10 Practice HUD: save states, SL, FA, Unranked (#39) |
| `C11_Details.dc.html` | C11 Game select: Details tab (#498, #805, #1050) |
| `C12_Collections.dc.html` | C12 Collections and favourites (#105) |
| `C13_Replays.dc.html` | C13 Replays: the local browser |
| `C14_ReplayPlayer.dc.html` | C14 Replay player |
| `C15_Browse.dc.html` | C15 Browse: catalog, downloads, removal list (#50) |
| `C16_Profile.dc.html` | C16 Profile with per-mode stats, pp and badge slots (#60) |
| `C17_SignIn.dc.html` | C17 Sign in: Google or Discord (#158) |
| `C18_SignInWaiting.dc.html` | C18 Sign in: finish in your browser (#159) |
| `C19_SessionExpired.dc.html` | C19 Session expired |
| `D1_ScriptError.dc.html` | D1 Script error, redesigned (#953) |
| `D2_Notifications.dc.html` | D2 Notifications: the overlay under the bell |
| `D3_NoticesInRun.dc.html` | D3 Notices in a run: leaderboard column |
| `D4_SortGroups.dc.html` | D4 Game select: Sort open, grouped by status |
| `D5_ListDim.dc.html` | D5 Game select: the list dims while the left side has focus |
| `D6_ListStates.dc.html` | D6 Game list: empty, no results, loading |
| `D7_Filters.dc.html` | D7 Game select: Status, Year and Tag filters (#805) |
| `D8_AutoRetry.dc.html` | D8 Auto-retry: footer or HUD (#773) |
| `D9_ModsHbRanked.dc.html` | D9 Mod select: Hitboxes ranked, Flashlight added, Mirror dropped (#554) |
| `D9b_ModsHbPractice.dc.html` | D9b Mod select: Hitboxes as a practice aid (#554) |
| `D10_ExitFlow.dc.html` | D10 Leaving a run in both modes (#1553) |
| `D11_DropImport.dc.html` | D11 Drag and drop import |
| `D12_Skip.dc.html` | D12 Skip cutscene hint and the frozen timer (#573) |
| `D13_PausePad.dc.html` | D13 Pause on a controller (#624) |
| `D14_PortStatus.dc.html` | D14 Port status marks (#62) |
| `E1_MenuDeck.dc.html` | E1 Main menu at 1280x800 (Steam Deck), controller hints (#117) |
| `E2_SelectDeck.dc.html` | E2 Time Attack select at 1280x800 (#117) |
| `E3_SettingsDeck.dc.html` | E3 Settings at 1280x800 (#117) |
| `E4_MenuJa.dc.html` | E4 Main menu in Japanese (#196) |
| `E5_SettingsJa.dc.html` | E5 Settings in Japanese (#196) |
| `E6_Colourblind.dc.html` | E6 Colourblind check of every meaning colour (#106) |
| `F1_RaceLobby.dc.html` | F1 Race lobby (#168), rough |
| `F2_RaceLive.dc.html` | F2 Race in progress: live ghosts (#169), rough |
| `F3_RaceResults.dc.html` | F3 Race results and finish order (#170), rough |
| `Toolbar.dc.html` | Toolbar (component) |
| `Hints.dc.html` | Button hint bar (component) |
| `MainMenu.dc.html` | Main menu (component) |
| `Select.dc.html` | Game select (component) |
| `Shelf.dc.html` | Classic shelf (component) |
| `Loading2.dc.html` | Loading card by mode (component) |
| `ClassicRun.dc.html` | Classic run (component) |
| `Session.dc.html` | Session summary (component) |
| `Opening.dc.html` | Opening and first run (component) |
| `MenuDialog.dc.html` | Dialogs over the main menu (component) |
| `Web.dc.html` | Web start and loading (component) |
| `Settings.dc.html` | Settings overlay (component) |
| `Bindings.dc.html` | Key bindings (component) |
| `Practice.dc.html` | Practice (component) |
| `Details.dc.html` | Details tab (component) |
| `Replays.dc.html` | Replays (component) |
| `Notices.dc.html` | Notifications (component) |
| `ModSheet.dc.html` | Mod select variants (component) |
| `Race.dc.html` | Races (component) |
| `Backdrop.dc.html` | Background system (from m4) |
| `GameScene.dc.html` | Placeholder game room (from m4) |
| `Leaderboard.dc.html` | Leaderboard (from m4) |
| `RunInfo.dc.html` | Run info (from m4) |
| `PauseBox.dc.html` | Pause box (from m4, items prop added) |
| `ProfileCard.dc.html` | Player card (from m4, mode prop added) |
