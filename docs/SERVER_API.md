# EmbroideryCraft Library: HTTP API (v1)

The library's web page uses this API, and so can any other program (Odoo, the machine stations,
scripts). Everything is JSON under `/api/v1`, except file uploads (multipart forms), file
downloads and preview pictures.

## Signing in
- **Programs** use an API token: an administrator makes one under **Settings › API tokens**, and
  the program sends it with every request:
  ```
  Authorization: Bearer ect_…
  ```
  A token acts as the person who made it. Revoking it stops it at once.
- **The web page** signs in with `POST /session` and keeps a cookie. Requests that change something
  with the cookie must also send the header `X-EC: 1` (another web site can't add it, so it can't
  act on your behalf).

Errors come back as `{"error": "…"}` with a 4xx or 5xx status; 401 means sign in first.

## Designs
A design groups files that share a name (`KARATE.DST`, `KARATE.EMB`, `karate.jpg`). Stitches,
size and counts come from a DST or EmbroideryCraft file; every other file is kept as it is.

| Method and path | What it does |
|---|---|
| `GET /designs?q=&tags=&customer=&kind=&usage=&archived=&sort=&offset=&limit=` | Search. `q`: words that must all appear in the name, notes, tags, file names or customer (name or old number). `tags`: comma-separated tags the designs must all have. `customer`: a customer's id, with the customers inside it (`no_customer=true` for designs without one). `kind`: `artwork` or `name`. `usage`: a usage name. `archived`: left out by default (archived designs and those of closed customers, unless a `customer` is given); `all` or `only`. `sort`: `recent` (default), `name`, `stitches`, `size`. `limit` up to 500 (default 60). Returns `{"total": n, "items": [design…]}`. |
| `POST /designs` | Add designs: a multipart form with one or more `file` fields and optional `tags` (comma-separated), `customer_id`, `usage` and `kind` fields. Files sharing a name become one design. Returns a report (below). |
| `GET /designs/{id}` | One design, with its `files`. |
| `PATCH /designs/{id}` | Change any of `name`, `notes`, `tags` (a list, replaces them all), `threads` (a list as long as the design's colours), `needles` (one per colour, or `[]`), `speed` (stitches a minute, `null` for the shop default), `customer_ids` (the design's customers in order, replacing them all; `[]` for none), `kind`, `usage`, `archived`, `background` (the fabric of the main colours, `#rrggbb`, `""` for none). Returns the design. |
| `DELETE /designs/{id}` | Remove the design and its files. |
| `POST /designs/{id}/files` | Add files to the design (multipart, `file` fields). Returns a report. |
| `GET /designs/{id}/preview?size=320&colourway=` | Preview as PNG: `320` (card) or `1000` (large), in the main colours or a colourway's, on a transparent background (the page puts the fabric colour behind it). Drawn from the stitches, or from a picture file if there are none. |
| `GET /designs/{id}/machine-file?format=tbf&colourway=` | The design for the machine: `tbf` (Tajima TBF with the needle of each colour, from the colourway or the main colours; needles not chosen follow "same thread, same needle") or `dst` (the stored DST as it is). |
| `POST /designs/{id}/colourways` | `{"name", "background"}`: a new colourway on that fabric (`#rrggbb`, optional), copying the main colours and needles. Returns the design. |
| `PATCH /colourways/{id}`, `DELETE /colourways/{id}` | Change `name`, `threads`, `needles`, `background` of a colourway, or remove it. Return the design. |
| `GET /files/{id}` | Download a stored file, under its original name. |
| `DELETE /files/{id}` | Remove one file. Returns `{"design_kept": bool}`; a design left with no files is removed. |
| `GET /tags` | Every tag with its number of designs: `[{"tag": "karate", "count": 12}, …]`. |

A design:
```json
{
  "id": 5, "name": "C70893HUG", "notes": "", "created": 1793835834, "updated": 1793835853,
  "width_mm": 95.2, "height_mm": 95.2, "stitches": 33733, "colours": 12, "trims": 0, "jumps": 92,
  "threads": [{"rgb": "#1f3a93", "name": "Blue", "code": ""}],
  "tags": ["karate", "shop"],
  "customers": [{"id": 12, "name": "Karaté Soleil Sainte-Julie (KSO1A2)"}, {"id": 40, "name": "Dojo Nord"}],
  "kind": "artwork", "usage": "Back", "archived": false,
  "needles": [1, 4, 2], "speed": null, "background": "#ffffff",
  "estimate": {"speed": 800, "sew_minutes": 43.4, "minutes_per_piece": 44.4, "cost": 35.08, "price": 33.73, "currency": "$"},
  "colourways": [{"id": 3, "name": "On black", "threads": […], "needles": [7, 4, 2], "background": "#141414"}],
  "preview": true,
  "files": [{"id": 9, "name": "C70893HUG.DST", "kind": "stitches", "size": 102027, "sha256": "…", "added": 1793835834}]
}
```
`kind` is `stitches` (DST), `project` (EmbroideryCraft), `picture` or `other`. Times are Unix
seconds. Stitch facts are `null` for a design without a stitch file.

An upload report:
```json
{"designs": [7], "added": ["sign.jpg"], "skipped": [{"name": "copy.dst", "reason": "already in the library, in \"fj\""}], "warnings": []}
```
`skipped` files were not stored (already in the library, empty, too large). `warnings` files were
stored with something to check (a `.dst` that is really a PDF is kept as a plain file).

## Customers and usages
A design can be used by several customers (one logo for a whole franchise network, a crest
shared by two clubs); `GET /designs?customer=` finds it under each. Customers can sit inside a group (a franchise network, a
federation), and keep the **old customer number** of the previous accounting software
(`KSO1A2`), which searches find too. A design is **artwork** (logos, crests, lettering) or a
**name** (one person's name on a belt or a bag), has a **usage** from the shop's list (Belt,
Back, Bag…), and can be **archived**.

| Method and path | What it does |
|---|---|
| `GET /customers` | Every customer: `id, name, old_number, parent_id, notes, archived, designs, children`. |
| `GET /customers/{id}` | One customer. |
| `POST /customers` | `{"name", "old_number", "parent_id", "notes"}` (a name or an old number). |
| `PATCH /customers/{id}` | Change `name`, `old_number`, `parent_id` (`null` takes it out of its group), `notes`, `archived` (a closed customer). |
| `DELETE /customers/{id}` | Administrators. Its designs stay, without that customer. |
| `POST /customers/old-numbers` | `{"text": "KSO1A2, Name\n…"}`: names customers by their old number. Returns `{"renamed", "unknown", "errors"}`. |
| `GET /usages` | The usage list: `id, name, words, names, designs`. `words` are the folder names meaning it; `names` makes its designs names. |
| `POST /usages`, `PATCH /usages/{id}`, `DELETE /usages/{id}` | `{"name", "words", "names"}`; renaming a usage renames it on its designs. Deleting is for administrators. |

Importing folders with `--customers` reads them as customer folders, possibly in A–Z folders:
`K/Karate Soleil [GROUPE]/Franchisés/KSO1A2/Ceinture/BOUTS/ÉMILE.DST` gives customer KSO1A2
in the group "Karate Soleil", usage "Belt end", a name. Folders whose name holds "Archives",
"Anciens" or "Old" archive their designs; a customer folder with "Fermé" or "Faillite" is a
closed customer, and a design used only by closed customers counts as archived. The same file
found under another customer becomes that customer's design too; generic folders ("Broderie", "Franchisés", "DST") are dropped; every other
folder name becomes a tag.

## Example: add a design from a script
```
curl -H "Authorization: Bearer $TOKEN" -F file=@KARATE.DST -F file=@KARATE.EMB -F tags=karate,belts \
     http://library.local:8080/api/v1/designs
```

## Threads
Thread charts are the brands' colour cards. **Isacord 40** (391 colours, numbers from AMANN's
colour card, names from the supplier's list) comes built in; the shop can add its own charts. A
design's colours refer to a chart's thread by `chart` and `code` in its `threads`.

| Method and path | What it does |
|---|---|
| `GET /thread-charts` | Every chart: `id, name, brand, notes, builtin, in_use, default, count, favourites`. |
| `POST /thread-charts` | New chart: `{"name", "brand", "notes"}`. |
| `PATCH /thread-charts/{id}` | Change `name`, `brand`, `notes`, `in_use` (offered when choosing threads), or `{"default": true}` (the shop's main chart). |
| `DELETE /thread-charts/{id}` | Administrators; built-in charts can only be hidden (`in_use: false`). |
| `POST /thread-charts/{id}/import` | `{"csv": "…"}`: one thread per line, number, `#rrggbb` and an optional name, comma, semicolon or tab separated. Existing numbers are updated. Returns `{"added", "updated", "errors"}`. |
| `GET /thread-charts/{id}/export` | The chart as CSV: `number,name,colour,favourite`. |
| `GET /threads?chart=&q=&favourites=true&near=%23rrggbb&limit=` | Threads of one chart (or of every chart in use): `q` searches numbers and names, `near` sorts by closeness to a colour and adds `delta` (CIEDE2000; under 3 is a close match). |
| `POST /threads` | `{"chart_id", "code", "name", "rgb"}`. |
| `PATCH /threads/{id}` | Change `code`, `name`, `rgb`, or `favourite` (true or false). |
| `DELETE /threads/{id}` | Remove a thread from its chart. |
| `POST /designs/{id}/match` | `{"chart_id"}` (or `{}` for the default chart): every colour of the design gets the closest thread. |

To give one colour of a design a thread, `PATCH /designs/{id}` with its whole `threads` list, the
chosen entry being `{"rgb", "code", "name", "chart"}` copied from the thread.

## Settings
| Method and path | What it does |
|---|---|
| `GET /settings` | `{"needle_count", "costing"}`. |
| `PATCH /settings` | Administrators: `needle_count` (needles per head, for TBF files) and/or `costing`: `default_speed`, `heads`, `colour_change_seconds`, `trim_seconds`, `handling_seconds`, `hourly_rate`, `thread_per_1000`, `backing_per_piece`, `price_per_1000`, `minimum_price`, `currency`. |

A design's `estimate`: sewing time of one run = stitches ÷ speed + colour changes and trims;
per piece, the run is shared by the heads and handling is added; cost = time × hourly rate +
thread + backing; price = stitches × price per 1000, at least the minimum.

## Backups (administrators)
| Method and path | What it does |
|---|---|
| `GET /backup` | `{"plan": {"folder", "every_hours", "keep"}, "last", "last_good", "running", "copies", "library"}`. `last` is the last attempt (`at`, `ok`, `message`, `database`, `files`, `copied`); `copies` the database copies in the folder, oldest first. |
| `PATCH /backup` | The plan: `folder` (full path, outside the library; empty: off), `every_hours` (1–720), `keep` (1–1000 database copies). |
| `POST /backup` | Back up now, in the background; poll `GET /backup` until `running` is false. |

The server checks every 10 minutes and backs up when `every_hours` have passed since the last good
backup (or an hour after a failed one), so a computer switched off at night catches up when it starts.

## Machines and design sticks

| | |
|---|---|
| `GET /machines` | The machines: `name`, `brand`, `model`, `type_code` (on the maker's plate), `serial`, `needles`, `format` (`dst` or `tbf`), `threads` (the thread usually on each needle, needle 1 first; `rgb` empty when not set), `stick` (`user@host` of its design stick), `stick_via` (a computer to go through, rarely needed), `notes`. |
| `POST /machines`, `PATCH /machines/{id}`, `DELETE /machines/{id}` | Add, change, delete (administrators). |
| `GET /machines/key` | The library's own ssh key (administrators). Add it once to each stick's `~/.ssh/authorized_keys`. |
| `GET /machines/stick-card` | Administrators: the next free stick name (`{"name": "ecstick2", "user": "shop"}`). |
| `POST /machines/stick-card` | Administrators: `{"name": "ecstick2", "ssid": "…", "wifi_password": "…", "country": "CA", "timezone": "America/Toronto", "keys": ""}` gives `ecstick2-card.zip`, to copy onto the `bootfs` drive of a card just written with Raspberry Pi OS Lite (64-bit). On its first start the Pi joins the Wi-Fi, takes the name, lets in the library's key (and `keys`, one public key per line; no password), installs the stick, takes the Wi-Fi password off the card and restarts. The password is only put in the zip. The library forgets the old host key of `name.local`. |
| `POST /machines/stick-probe` | Administrators: `{"target": "shop@ecstick2.local", "via": ""}`: the stick's status, as for a machine's stick, before its machine is made. |
| `GET /machines/{id}/stick` | What is on the stick: `attached` (the machine is using it), `settings`, `designs` (file names). |
| `POST /machines/{id}/needle-plan` | `{"design": 302, "colourway": null}`: the needle of each colour on this machine. `steps`: `thread`, `needle`, `how` (`set` on the design, `same` thread on the machine, `closest` colour, `order` when the machine has no threads set), `on_needle` (the machine's thread there), `ok` (it is the same thread); `ok` overall. |
| `POST /machines/{id}/stick` | Send designs: `{"designs": [{"design": 302, "colourway": null, "needles": [2, 5]}], "replace": false}`. They go in the machine's format. `needles` (one per colour, as checked with `needle-plan`) are used as given; without them every colour's thread must be on the machine, or the send is refused. A TBF goes with its `.ct0` condition file, which is where the machine reads the needles. Answers `{"sent": [names], "stick": {…}}`; the stick's `designs` leave out `.ct0`/`.dgf` files, and removing a `.tbf` removes them too. |
| `POST /machines/{id}/stick/remove` | `{"names": ["AVG-WCA.DST"]}` takes designs off the stick. |
| `POST /machines/{id}/stick/clear` | Empties the stick. |
| `POST /designs/{id}/needles-from-machine` | `{"machine": 1, "colourway": null}` chooses the needles from the machine: the needle with the same thread, else the closest colour. |

A stick that cannot be reached answers 502 with a message. The machines' largest needle count also
raises the needle limit for colourways (Settings › Needles per head).

## People and tokens (administrators)
| Method and path | What it does |
|---|---|
| `GET /status` | No sign-in needed: product, version, `setup_needed`, and the signed-in user if any. |
| `POST /setup` | `{"name", "password"}`: the first administrator, only while there is no user. |
| `POST /session`, `DELETE /session` | Sign in (`{"name", "password"}`) and out (web page). |
| `GET /me`, `POST /me/password` | Who is signed in; change one's own password (`{"old", "new"}`). |
| `GET /users`, `POST /users`, `DELETE /users/{id}` | People: `{"name", "password", "admin"}`. |
| `GET /tokens`, `POST /tokens`, `DELETE /tokens/{id}` | API tokens: `POST {"name"}` returns `{"id", "token"}`; the token is shown only then. |

## Jobs and connections

| Call | What it does |
|---|---|
| `GET /jobs?days=7` | Jobs not finished, and those finished in the last days: `name`, `design_id`/`design_name`, `colourway_id`/`colourway_name`, `quantity`, `customer`, `machine_id`, `due`, `status` (`waiting`, `started`, `done`, `cancelled`, `problem`), `reference`, `design_hint` (the outside name of the design, e.g. the Odoo operation), `message` (what happened last), `loaded`, `source` (the connection). |
| `POST /jobs`, `PATCH /jobs/{id}`, `DELETE /jobs/{id}` | Make, change (a `null` clears a field) or delete (admin) a job. |
| `GET /jobs/{id}/design` | Designs sharing a code with the job's outside name (`suggestions`, full designs), and `can_link` (the job came from a connection). |
| `POST /jobs/{id}/design` | `{"design_id", "colourway_id", "every"}`: the job's design and colours; `every` links the outside name to them for every job of it. |
| `POST /jobs/{id}/load` | Put the job's design on its machine's stick now (replacing what is there). |
| `GET/POST /connections`, `PATCH/DELETE /connections/{id}` (admin) | Connections (`kind: "odoo"`, `settings: {url, database, notes}`, `secret`: the API key, never sent back; `has_secret` says one is saved). |
| `POST /connections/{id}/test`, `POST /connections/{id}/sync` | Check the connection; ask for the work now (`seen`, `new_jobs`, `loaded`, `problems`). The library also asks every `poll_seconds`. |
| `GET/PUT /connections/{id}/machines` | The other system's work centers and the machine linked to each. |
| `GET/PUT /connections/{id}/designs` | Operations on the linked work centers and the design linked to each (with suggestions). |
| `GET /connections/{id}/customers` | The library's customers beside the other system's (Odoo: companies and people that buy, not suppliers): `matches` (each `customer` with `linked`, a `sure` match and `how` — `old number` or `name` — or `maybe`, the likely ones) and `remote` (every outside customer: `remote`, `name`, `old_number`, `reference`, `rank`). The old number is read from the partner field labelled "Acomba" (or "old number"), or the one named in the connection's settings as `old_number_field`. |
| `PUT /connections/{id}/customers` | `{"links": [{"customer_id": 72, "remote": {…an outside customer…}, "take_name": false}]}`; `"remote": null` unlinks. A library customer named only by its old number takes the outside name (any one does with `take_name`), and one without an old number takes the outside one. Work from the connection for a customer not linked links the surely same library customer, or adds it; the job keeps it as `customer_id` (`customer_label` for people), and `GET /jobs/{id}/design` adds its designs as `customer_designs`. |
| `GET /designs/{id}/published`, `POST /designs/{id}/publish` (admin) | The design as an Odoo product: `EMB-<design>`, a "Colourway" variant for the main colours and each colourway (`EMB-<design>-<colourway>`), the preview as picture, the estimate's cost on each variant and price on the product. Added to a nomenclature and "consumed in" an operation, it is the design of that operation's work orders. |

When a work order on a linked work center starts (Odoo state `progress`, e.g. from Shop Floor),
the job's design is loaded on its machine's stick: the design is the `EMB-` component consumed in
the work order's operation, else the design linked to the operation, else the one chosen on the
job. The colours not on the machine's needles get the closest needle and are noted on the job.

## Running it
```
embcraft-server [--data <folder>] [--listen 127.0.0.1:8080] [--inbox <folder>]
embcraft-server import <folders or files>… [--customers] [--tag <tag>]… [--only dst,emb]
embcraft-server backup <backup folder> [--keep 30]
embcraft-server restore <backup folder> --data <new folder> [--from library-….db] [--list]
embcraft-server password <user>
```
`--listen 0.0.0.0:8080` lets other computers in the shop reach it. The library (the `library.db`
database, the stored files and previews) lives in `--data`, `$EMBCRAFT_DATA`, or the user's
application data folder.

`--inbox <folder>` (or `$EMBCRAFT_INBOX`): designs saved into that folder go into the library by
themselves, tagged `shared folder`, once each file has stopped changing for 6 seconds; files of
one name make one design, and the files leave the folder. Machine files that do not read go to
`Not imported/` with a `.txt` note. `station/library-vm/shared-folder.sh` makes it a network
share (Samba) on the server.

Backups (Settings › Backups, or `embcraft-server backup`, safe while the server runs) go into a
folder off the library's disk: `databases/library-<date-time>.db`, one consistent copy of the
database per backup, the newest `keep` of them; and `files/`, every design file by content, checked
as it is copied and never removed, so a design deleted by mistake can still be restored. A database
copy is written only once all of its files are there. Previews are not backed up; they are drawn
again. `restore` checks the database copy and every file, and only writes into a folder that holds
no library yet. The database copy holds the people and their password hashes: keep the folder
where only the shop can reach it.
