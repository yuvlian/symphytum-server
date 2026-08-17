# symphytum-server

hololive Dreams private server

## General Showcase

<details>
<summary>Screenshots</summary>

-- --

| | |
|---|---|
| ![1](.screenshots/20260807003842_1.jpg) | ![2](.screenshots/20260807004135_1.jpg) |
| ![3](.screenshots/20260807004324_1.jpg) | ![4](.screenshots/20260807004337_1.jpg) |
| ![5](.screenshots/20260807004413_1.jpg) | ![6](.screenshots/20260807004447_1.jpg) |
| ![7](.screenshots/20260807004524_1.jpg) | ![8](.screenshots/20260807004804_1.jpg) |
| ![9](.screenshots/20260807004850_1.jpg) | ![10](.screenshots/20260807005003_1.jpg) |
| | |

</details>

-- --

<details>
<summary>Features</summary>

-- --

- **Auth / account**: automatically creates acccount on login and is maxed out account (cards, characters, costumes, missions, music, skill trees, profiles, balances, items, memberships, etc.); unique for every device too

- **Park**: enter/refresh, change park character

- **Character**: full character history, voice etc.

- **Costume**: costume customization

- **Gacha**: gacha draw, probabilities, master-data driven banners and button costs (free/paid stones, tickets), gacha-point bonuses, bloom-aware duplicate compensation

- **Exchange**: master-data driven booths (gacha point <-> R5 cards, membership emblem/stamp booths), purchase limits, read/purchase persistence.

- **Live**: deck save/get/draft with derived Unit Score (though not 1:1 with original server), start/finish single player with score-evaluation ranks. best/cleared is tracked.

- **Card**: parameters (level / potential / skill tree), level & potential RPCs.

- **Skill tree**: release / connect / reset nodes; the snapshot keeps connected groups and cards index-aligned (the client throws otherwise).

- **Profile**: name / bio / fan mark / emblems / custom palettes. (custom palettes kinda broken tho)

- **Jump rope**: start / finish single with best/cleared tracking

- **Notice / Announcement / Gift**: fake notice catalog (check it out hehe), empty announcement/gift.

- **Shop / Membership**: fake stone shop + membership subscription items :3

- **Misc**: notifications, startup notifications, system info, multi-game stubs, user-content CDN signed cookie (required for custom-palette image downloads).

- **http-server**: `/palette/{n}` placeholder image, `/palette_upload/{token}` PUT/GET, `/notice/{id}` fake notice pages.

- **master updater**: you can easily update & dump master data from original server.
</details>

-- --

<details>
<summary>Known Limitations</summary>

-- --

**Game modes & multiplayer**
- No multiplayer at all: live multi, jump-rope multi, private rooms, matchmaking, and the MultiGame service are stubs.

- Only two game modes work: **single live** and **single jump rope**. Everything else (marathon, combo card, cooking puzzle, splash ball, circuit, chase, music creative chart, mini games) is unimplemented.

**Economy & rewards**
- Finishing a live or jump rope does not consume resources (no stamina / reward-up stamina consumption) or grant rewards the way the original does. Jump-rope rewards are simplified. Live finish records scores/ranks but grants no reward items.

- No real progression economy: accounts are seeded maxed (all cards, characters, costumes, missions, skill-tree points, balances). Card exp/level-up, potential upgrade, and limit-break RPCs are unimplemented.

- Shop is a fake catalog (fake prices/IAP ids, no actual purchases); `PurchaseConsumptionItem` is a no-op. Membership lists subscription items but there is no billing.

- Login bonus returns an empty default. Events are empty. Gifts and announcements are empty. All hardcoded to be so.

**Scores & formulas**
- Unit score is not 1:1 with the original server. It's pretty ass to reverse since its entirely serversided.

- Score-evaluation ranks for live results use the master-data thresholds; music highest-score rating is simplified.

**UI / cosmetics**
- The gacha menu looks wonky, banner names come from a small hardcoded map, no precaution text, asset art, or pickup card art but drawing gacha works.

- Custom palettes only show the uploaded image in the editor preview (and in lobbies). Palette images always serve the placeholder `http-server/palette.jpg`. Palette base also doesn't save/load properly rn.

- Park's banners/notices are packet sniff derived, so they go stale when the real game updates its campaigns

**Unimplemented RPCs**
- Park: set accessory, open treasure, collect symbols, listen call, report action, read talk free, select area.

- Friend service, publish settings / blocking, other-user profile detail, custom palette upload/delete flows beyond the basic edit.

- MultiGame (multiplayer): list invited private rooms, ping server lists, etc.

**Data quirks**
- Skill tree state is seeded as "all nodes released + connected" for every character with derived connect-effect cards; visually not faithful but functional and crash-free (the client index-pairs connected groups with cards; the snapshot keeps them aligned)

- Exchange gacha-point reward pools are the first 5 R5 cards by master order, not the per-banner pools the real server uses; membership exchange serves emblem/stamp booths only.
</details>

## Setup (Windows)

<details>
<summary>Prerequisites</summary>

-- --

- **Rust**: https://rustup.rs/ Nightly toolchain & cranelift backend installed. You can also just remove cranelift from cargo.toml and codegen-backend entirely if you want to use stable.

- **OpenSSL**: https://slproweb.com/products/Win32OpenSSL.html. Make sure openssl is in path env.

- **protoc**: https://github.com/protocolbuffers/protobuf/releases/download/v35.1/protoc-35.1-win64.zip extract and be sure to put it in PATH environment variable

</details>

-- --

<details>
<summary>Running</summary>

-- --

### Everything below assumes PowerShell

1. Clone this repo: `git clone https://github.com/yuvlian/symphytum-server; cd symphytum-server`

2. Setup cert: `./cert`

3. Setup master data: `./bin master`

4. Start http server: `./bin http`

5. Start rpc server: `./bin rpc`

You can also add ` -r` after the `./bin x` stuff if you want to compile with release profile. Modify `config.toml` as needed.

</details>

-- --

<details>
<summary>Playing</summary>

-- --

1. Download https://github.com/yuvlian/symphytum/releases/download/0.1.0/win-x64.7z

2. Extract the content to same folder as `hololive-Dreams.exe`

3. Rename dll file to `umpdc.dll`

4. Run game

</details>

## Project Meta

<details>
<summary>Structure</summary>

-- --

```
symphytum-server/
├── cert.ps1              # script to gen & install certs
├── bin.ps1               # helper script to run bin crates
├── config.toml           # rpc-server / http-server / inspector / cert / log settings
├── database/             # database crate
│   ├── migrations/       # 3NF DDL
│   └── src/              # account, inventory, live, jump_rope, user_data, models
├── resource/             # common resource crate
│   ├── src/              # master data loader/updater, config, cert, quali crypt
│   └── master/           # *.json master data + master_get.bin
├── rpc-server/           # tonic gRPC server (main PS)
│   └── src/
│       ├── main.rs       # service registration
│       └── services/     # one dir per gRPC service (mod.rs + logic + delta.rs)
├── http-server/          # for fake notices & placeholder img palette
├── inspector/            # packer sniff viewer site
│   └── sniffs/           # captured game traffic (.bin)
└── types/                # protobufs crate
```

</details>

-- --

<details>
<summary>License</summary>

-- --

MIT

</details>
