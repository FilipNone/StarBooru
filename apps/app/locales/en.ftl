# StarBooru UI text, English (the reference catalogue).

posts-count = { $count ->
    [one] { $count } post
   *[other] { $count } posts
}

## Shared

retry = Retry
loading = Loading
notfound-title = Nothing here
notfound-body = There is no page at { $path }.
notfound-back = Back to the gallery
theme-system = System
theme-light = Light
theme-dark = Dark

## Time

ago-now = just now
ago-minutes =
    { $count ->
        [one] 1 minute ago
       *[other] { $count } minutes ago
    }
ago-hours =
    { $count ->
        [one] 1 hour ago
       *[other] { $count } hours ago
    }
ago-yesterday = yesterday
ago-days = { $count } days ago
date-short = { $day } { $month } { $year }
month-1 = Jan
month-2 = Feb
month-3 = Mar
month-4 = Apr
month-5 = May
month-6 = Jun
month-7 = Jul
month-8 = Aug
month-9 = Sep
month-10 = Oct
month-11 = Nov
month-12 = Dec

## Navigation

nav-main = Main
nav-gallery = Gallery
nav-explore = Explore
nav-tags = Tags
nav-upload = Upload
nav-settings = Settings
search-placeholder = Search tags, e.g. blue_sky -cloud rating:g
search-label = Search tags

## Sign-in

login-unreachable = Cannot reach the server: { $error }
login-wrong = Wrong username or password.
login-tagline = Your private image library
login-first-user = This server has no accounts yet. The account you create now becomes the administrator.
login-server = Server
login-username = Username
login-password = Password
login-wait = Please wait…
login-create-admin = Create administrator account
login-create = Create account
login-sign-in = Sign in
login-have-account = I already have an account
login-want-account = Create an account

## Shared

load-more = Load more
sure = Sure?
not-signed-in = Not signed in
no-matches = No matches
nothing-matches = Nothing matches “{ $query }”.

## Recycle bin

trash-title = Recycle bin
trash-purged =
    { $count ->
        [one] Deleted 1 post for good.
       *[other] Deleted { $count } posts for good.
    }
trash-empty = Empty recycle bin
trash-empty-confirm = Delete everything for good?
trash-lead = Deleted posts wait here until you restore them or delete them for good. Deleting for good also removes the file from the server.
trash-none = The recycle bin is empty
trash-restore = Restore
trash-restore-post = Restore post { $id }
trash-purge = Delete for good
trash-purge-post = Delete post { $id } for good

## Explore

explore-window = Trending over
explore-today = Today
explore-week = Week
explore-month = Month
explore-year = Year
explore-phrase-today = today
explore-phrase-week = this week
explore-phrase-month = this month
explore-phrase-year = this year
explore-phrase-days = in the last { $count } days
explore-nothing = Nothing to explore yet
explore-nothing-body = Upload images or videos, and the best of them show up here.
explore-everything = Explore everything
explore-trending = Trending
explore-no-upvotes = No upvotes { $when }. Vote on posts you like and they show up here.
explore-popular-tags = Popular tags
explore-tags-note = on posts added { $when }
explore-added = Posts added { $when }
explore-on-this-day = On this day
explore-random = Random picks
explore-shuffle = Shuffle
explore-filter-placeholder = Narrow down, e.g. -rating:e
explore-filter-label = Narrow down with a search

## Shared

show-more = Show more
tags-count =
    { $count ->
        [one] 1 tag
       *[other] { $count } tags
    }
post-alt = Post { $id }
role-admin = admin
role-user = user

## Settings

settings-appearance = Appearance
settings-theme = Theme
settings-language = Language
settings-language-system = System
settings-accent = Accent colour
settings-accent-custom = Custom accent hue
accent-violet = Violet
accent-pink = Pink
accent-coral = Coral
accent-amber = Amber
accent-green = Green
accent-teal = Teal
accent-blue = Blue
accent-red = Red
settings-library = Library
settings-trash-note = Restore or permanently delete posts
settings-dup-note = Find posts that look alike and keep the best copy
settings-help-legal = Help and legal
settings-help-note = Query syntax, examples and keyboard shortcuts
settings-privacy-note = What is stored, where, and for how long
settings-terms-note = The rules for using this server
settings-account = Account
settings-signed-in-as = Signed in as
settings-role = Role
settings-server-version = Server version
settings-sign-out = Sign out
password-current = Current password
password-new = New password
password-too-short = At least { $count } characters.
password-repeat = Repeat the new password
password-mismatch = The passwords don't match.
password-changed = Password changed. Your other devices were signed out.
password-changing = Changing…
password-change = Change password
sessions-title = Signed-in devices
sessions-unknown = Unknown client
sessions-this-device = This device
sessions-times = Active { $active } · signed in { $created }
sessions-revoke-others = Sign out everywhere else
sessions-revoke = Sign out this device

## Duplicates

dup-title = Duplicate finder
dup-matching = Matching
dup-strict = Strict
dup-normal = Normal
dup-loose = Loose
dup-lead = Posts that look alike, closest matches first. Keep the best copy and the others go to the recycle bin.
dup-unhashed =
    { $count ->
        [one] 1 post can't be compared yet: it has no thumbnail, or is still being fingerprinted.
       *[other] { $count } posts can't be compared yet: they have no thumbnail, or are still being fingerprinted.
    }
dup-none = No duplicates found
dup-none-try-loose = Try Loose matching to catch edited copies.
dup-none-loose = Nothing here looks alike.
dup-groups =
    { $count ->
        [one] 1 group
       *[other] { $count } groups
    }
dup-some-of = { $shown } of { $count } posts
dup-keep-which = Post to keep
dup-merge = Copy their tags to the kept post
dup-dismiss = Not duplicates
dup-resolve =
    { $count ->
        [one] Keep one, bin the other
       *[other] Keep one, bin { $count } others
    }
dup-unknown-size = Unknown size
dup-keep = Keep
dup-identical = Look identical
dup-nearly-identical = Nearly identical
dup-very-similar = Very similar
dup-similar = Similar

## Ratings

rating-general = general
rating-sensitive = sensitive
rating-questionable = questionable
rating-explicit = explicit

## Search cheat sheet

help-title = Search cheat sheet
help-try = Try a query
help-try-label = Query to explain
help-search = Search
help-search-for = Search for { $term }
help-tags = Tags
help-tag = has the tag
help-tag-all = has every tag listed
help-tag-not = doesn't have the tag
help-tag-spaces = two tags: spaces separate terms, so write blue_sky
help-tag-meta = high-resolution posts without sound. The server adds meta tags like lowres, highres, absurdres, wide_image, tall_image, animated, video, sound and long_video by itself
help-filters = Filters
help-rating = rated general (g), sensitive (s), questionable (q) or explicit (e)
help-rating-any = any of the listed ratings
help-rating-not = any rating but explicit
help-type = still images; also type:animation and type:video
help-deleted = posts in the recycle bin
help-text = Text in images
help-text-word = the text read in the image has the word (any case)
help-text-phrase = has all these words; quotes keep them together
help-text-not = the text read in the image doesn't have the word
help-sorting = Sorting
help-order-new = newest first (the default); order:old for oldest
help-order-score = highest score first
help-order-size = largest file first
help-order-pixels = highest resolution first
help-order-tags = most tags first
help-order-updated = most recently edited first
help-order-direction = add _asc or _desc to set the direction
help-order-random = a new shuffle every time
help-combined = Combined
help-combined-cats = safe cats without dogs, best first
help-combined-videos = the biggest videos
help-tips = Good to know
help-tip-normalise = Tags are lower-cased, and spaces or repeated underscores become one _.
help-tip-aliases = An alias finds its tag, and implied tags are added when you tag a post.
help-tip-terms = A query holds at most 40 terms.
help-tip-saved = Star a search in the gallery to keep it as a saved search.
help-shortcuts = Keyboard shortcuts
key-prev-next = previous or next post
key-edit = edit tags and rating
key-fullscreen = full screen
key-zoom = zoom in or out
key-fit = fit to the screen
key-escape = reset zoom, leave full screen, then back to the gallery
key-double-click = Double click
key-double-click-action = zoom in on a point, again to fit
key-shift-click = Shift + click
key-shift-click-action = select a range in selection mode
explain-deleted = In the recycle bin
explain-tagged = Tagged { $tag }
explain-tagged-all = Tagged with all of { $tags }
explain-not-tagged = Not tagged { $tag }
explain-tagged-none = Tagged with none of { $tags }
explain-rated = Rated { $ratings }
explain-not-rated = Not rated { $ratings }
list-or = { $rest } or { $last }
explain-images = Still images only
explain-animations = Animations only
explain-videos = Videos only
explain-text = Text in the image has “{ $text }”
explain-not-text = Text in the image doesn’t have “{ $text }”
explain-every = Every post
explain-order-new = Newest first
explain-order-old = Oldest first
explain-order-random = In random order
explain-order-score-desc = By score, highest first
explain-order-score-asc = By score, lowest first
explain-order-size-desc = By file size, largest first
explain-order-size-asc = By file size, smallest first
explain-order-pixels-desc = By resolution, highest first
explain-order-pixels-asc = By resolution, lowest first
explain-order-tags-desc = By tag count, most first
explain-order-tags-asc = By tag count, fewest first
explain-order-updated-desc = By last edit, latest first
explain-order-updated-asc = By last edit, earliest first

## Privacy Policy and Terms of Service

privacy-1 = StarBooru is software that runs on a server chosen by you or someone you trust. Nothing you store is sent to the StarBooru project or to anyone else.
privacy-2 = Who is responsible
privacy-3 = The person or organisation that runs this server (the operator) decides who may sign up and is responsible for the data on it. Questions about your data go to them.
privacy-4 = What the server stores
privacy-5 = Your account: username, role and a salted Argon2id hash of your password. The password itself is never stored. If you sign in through the operator’s identity provider, the server keeps that provider’s id for you and the username or email it reports, to recognise you next time.
privacy-6 = Your sessions: a hash of each sign-in token, the kind of device it came from (for example “StarBooru app on Linux” or “Firefox on Android”), when it was created and last used, and when it expires. API keys you create are kept the same way: a hash of the key, its name, and when it was last used.
privacy-7 = What you add: uploaded files with their size, dimensions, checksum and a visual fingerprint for the duplicate finder, plus tags, ratings, sources, votes and saved searches. For each share link you make: the posts in it, its title, when it expires, and how many times and when it was last opened (not by whom).
privacy-8 = To slow down password guessing, the server counts failed sign-ins per account and per network address, and sign-ups per address. These counts are kept in memory for at most an hour and never written to disk.
privacy-9 = What stays on your device
privacy-10 = The app remembers the server address, your sign-in token and your preferences, such as the theme. Signing out removes the token.
privacy-11 = What StarBooru does not do
privacy-12 = No analytics, advertising or tracking, and no third-party services beyond an identity provider the operator chooses to set up.
privacy-13 = The server only contacts other sites when you import from a URL, and the operator’s identity provider when you sign in through it. Imports carry a generic user agent and none of your details. If an administrator sets up an AI provider, thumbnails of posts are sent to it to suggest tags, and the text read in an image when you translate it; a provider on the operator’s own network keeps them there.
privacy-14 = File contents, passwords and tokens are never written to the server's logs.
privacy-15 = Who can see your posts
privacy-16 = Everyone with an account on this server can see the posts on it. Media links handed to the app are signed and expire, so a copied link stops working within 12 hours. If you make a share link, anyone who has it can see the posts in it, without their tags, until it expires or you remove it.
privacy-17 = Keeping and deleting
privacy-18 = Deleted posts go to the recycle bin. They are erased when an administrator empties it, or automatically after the period the operator sets.
privacy-19 = Sessions end when you sign out, when they expire, or when you sign out other devices in Settings. Changing your password signs out every other device.
privacy-20 = Administrators of this server see each account’s username, role, when it joined and was last active, and how much it has uploaded. They can set a storage limit, reset a forgotten password, disable an account or delete it. To delete your account, ask one of them.
privacy-21 = The operator may keep backups, which follow their own schedule.
terms-1 = These terms cover your use of this StarBooru server. The operator who runs it may add to them.
terms-2 = Your account
terms-3 = Keep your password to yourself, and sign out devices you no longer use (Settings › Signed-in devices).
terms-4 = The first account on a server is its administrator. Administrators can manage tags, aliases and implications, empty the recycle bin, change the server’s settings, and manage accounts: roles, storage limits, password resets, disabling and deleting.
terms-5 = Your content
terms-6 = Upload only what you have the right to keep and share with the other people on this server, and what is legal where the server runs.
terms-7 = You keep every right to what you upload. You let the server store it, make thumbnails and previews, and show it to the accounts on this server.
terms-8 = Originals are kept byte for byte. Rate posts honestly, so that rating filters work for everyone.
terms-9 = Fair use
terms-10 = Don't try to reach other people's accounts or data, or to get around the server's limits.
terms-11 = Don't overload the server with automated requests.
terms-12 = No warranty
terms-13 = StarBooru is provided as is, without warranty of any kind. Keep your own copy of anything you can't afford to lose. The operator can suspend accounts that break these terms and can change them; the current version is always on this page.

## Legal links

privacy-title = Privacy Policy
terms-title = Terms of Service
legal-nav = Legal
legal-back = Back to StarBooru
legal-privacy-short = Privacy
legal-terms-short = Terms

## Upload

upload-skipped =
    { $count ->
        [one] Skipped 1 file that is not an image or video.
       *[other] Skipped { $count } files that are not images or videos.
    }
upload-urls-only = Enter addresses that start with http:// or https://.
upload-drop = Drop images or videos here
upload-or-choose = or click to choose files
upload-folder = Choose a folder
upload-folder-tags = Tag files with their folder name
upload-urls-placeholder = Paste image or video addresses
upload-urls-label = Image or video addresses
add = Add
upload-shared-tags = Tags for all files
rating = Rating
upload-clear = Clear list
upload-running = Uploading…
upload-start =
    { $count ->
        [0] Upload
        [one] Upload 1 file
       *[other] Upload { $count } files
    }
upload-from-web = From the web
upload-queued = Queued
upload-uploading = Uploading
post-number = Post #{ $id }
upload-failed = Failed
upload-file-tags-placeholder = Tags for this file only
upload-file-tags-label = Tags for { $name }
upload-unreadable = cannot read file: { $error }

## Tags

category-general = General
category-artist = Artist
category-character = Character
category-copyright = Copyright
category-meta = Meta
tags-sort = Sort tags
tags-most-used = Most used
tags-a-z = A–Z
tags-filter-label = Filter tags
tags-filter-placeholder = Filter by name or alias
tags-none-yet = No tags yet
tags-none-match = No tags match
tags-appear = Tags appear here once posts use them.
tags-missing = No tag called “{ $name }”
tags-all = All tags
tags-browse =
    { $count ->
        [one] Browse 1 post
       *[other] Browse { $count } posts
    }
tags-is-alias = “{ $name }” is an alias of this tag.
tags-category = Category
tags-aliases = Aliases
tags-aliases-hint = Other names for this tag. Tagging or searching with an alias uses this tag. Adding an alias that is already a tag merges it into this one.
tags-aliases-none = No aliases.
tags-implies = Implies
tags-implies-hint = Posts with this tag get these tags too, including posts tagged earlier.
tags-implies-none = Implies no other tags.
tags-implied-by = Implied by
tags-implied-by-hint = Tags that add this one.
tags-implied-by-none = No tag implies this one.
tags-new-alias = New alias
tags-implied-tag = Implied tag
tags-remove = Remove
tags-remove-named = Remove { $name }
tags-admins-only = Only admins can change this.

## Post

post-in-trash = This post is in the recycle bin.
post-upvote = Upvote
post-downvote = Downvote
post-download = Download
post-edit = Edit
post-edit-tags = Edit tags
post-delete = Delete
post-delete-title = Move to recycle bin
post-tags = Tags
cancel = Cancel
save = Save
post-no-tags = No tags yet.
post-details = Details
post-related = Related
post-size = Size
post-dimensions = Dimensions
post-length = Length
post-type = Type
post-added = Added
post-source = Source
post-prev-title = Previous (←)
post-prev = Previous post
post-next-title = Next (→)
post-next = Next post
zoom-out-title = Zoom out (−)
zoom-out = Zoom out
zoom-fit-title = Fit (0)
zoom-reset = Reset zoom
zoom-in-title = Zoom in (+)
zoom-in = Zoom in
fullscreen-exit-title = Exit fullscreen (F)
fullscreen-exit = Exit fullscreen
fullscreen-title = Fullscreen (F)
fullscreen = Fullscreen

## Gallery

gallery-tag-info = About this tag
gallery-stop-selecting = Stop selecting
gallery-select = Select posts
gallery-syntax = Search syntax
gallery-empty = No posts yet
gallery-empty-body = Upload images or videos to start your collection.
gallery-try-fewer = Nothing matches this search. Try fewer tags.
bulk-label = Edit selected posts
bulk-selected =
    { $count ->
        [one] 1 selected
       *[other] { $count } selected
    }
bulk-select-none = Select none
bulk-select-all = Select all
bulk-tags-placeholder = Tags: cat -dog
bulk-tags-title = Tags to add; put - before a tag to remove it
bulk-tags-label = Tags to add, or to remove with a leading minus
bulk-keep-rating = Keep rating
bulk-apply = Apply
bulk-delete-confirm-label =
    { $count ->
        [one] Delete 1 post?
       *[other] Delete { $count } posts?
    }
bulk-delete-confirm = Delete { $count }?
bulk-updated =
    { $count ->
        [one] Updated 1 post
       *[other] Updated { $count } posts
    }
bulk-moved =
    { $count ->
        [one] Moved 1 post to the recycle bin
       *[other] Moved { $count } posts to the recycle bin
    }
bulk-skipped =
    { $count ->
        [one] skipped 1 post
       *[other] skipped { $count } posts
    }
bulk-skipped-undeletable =
    { $count ->
        [one] skipped 1 post you can't delete
       *[other] skipped { $count } posts you can't delete
    }
bulk-done = { $done }.
bulk-done-skipped = { $done }; { $skipped }.
search-forget = Forget this search
search-save = Save this search
saved-searches = Saved searches
sort = Sort
sort-by = Sort by
sort-new = Newest
sort-old = Oldest
sort-score = Top score
sort-updated = Recently edited
sort-size = Largest file
sort-size-asc = Smallest file
sort-pixels = Highest resolution
sort-tags = Most tags
sort-tags-asc = Fewest tags
sort-random = Random

## API keys

keys-title = API keys
keys-lead = Let scripts and other tools use StarBooru as you. A key can't change your password, sign-ins or keys.
keys-copy-now = Copy the key “{ $name }” now. It won't be shown again.
keys-token = New API key
keys-copy = Copy
keys-copied = Copied
keys-none = No API keys yet.
keys-read-only = Read-only
keys-full = Full access
keys-expired = Expired
keys-used = Used { $when }
keys-never-used = Never used
keys-no-expiry = Never expires
keys-expired-on = Expired { $date }
keys-expires-on = Expires { $date }
keys-name = Name
keys-name-placeholder = What will use it, e.g. backup script
keys-access = Access
keys-lifetime = Expires
keys-never-expires = Never
keys-days =
    { $count ->
        [one] 1 day
       *[other] { $count } days
    }
keys-create = Create key
keys-delete = Remove key
keys-delete-named = Remove the key { $name }

## Administration

admin-title = Administration
admin-only = Only admins can open this page.
admin-tab-overview = Overview
admin-tab-accounts = Accounts
admin-tab-server = Server
admin-on = On
admin-off = Off
admin-posts = Posts
admin-images = Images
admin-animations = Animations
admin-videos = Videos
admin-bin = In the recycle bin
admin-tags = Tags
admin-accounts = Accounts
admin-media = Media on disk
admin-database = Database size
admin-uploads-title = Uploads, last 30 days
admin-uploads-total =
    { $count ->
        [one] 1 upload in the last 30 days
       *[other] { $count } uploads in the last 30 days
    }
admin-server-title = Server
admin-running = Started
admin-upload-limit = Largest upload
admin-bin-size = Recycle bin size
admin-disabled-accounts = Disabled accounts
admin-video-thumbs = Video thumbnails (ffmpeg)
admin-url-import = Import from URLs
admin-accounts-lead = Everyone with an account on this server, what they uploaded and when they were last active.
admin-disabled = Disabled
admin-you = You
admin-active = active { $when }
admin-never-active = never active
admin-joined = joined { $date }
admin-manage = Manage
admin-manage-named = Manage { $name }
admin-role-saved = Role changed.
admin-limit = Storage limit (GB)
admin-limit-none = No limit
admin-limit-hint = Uploads that would go past it are refused. Leave empty for no limit.
admin-limit-save = Set limit
admin-limit-saved = Storage limit saved.
admin-limit-invalid = Type a number of GB, or leave it empty.
admin-password-new = New password
admin-password-hint = For someone who forgot theirs. Signs them out everywhere; their API keys keep working.
admin-password-reset = Reset
admin-password-reset-done = Password reset. Tell them the new one.
admin-disable = Disable account
admin-enable = Enable account
admin-disabled-done = Account disabled and signed out.
admin-enabled-done = Account enabled.
admin-delete = Delete account
admin-delete-confirm = Delete for good?
admin-delete-note = Deleting removes the account, its votes, saved searches and keys. Its posts stay.
admin-server-lead = These apply at once and override the server's environment variables.
admin-name = Server name
admin-name-hint = Shown on the sign-in page.
admin-registration = Anyone can create an account
admin-registration-hint = Off: only the accounts that exist can sign in.
admin-trash-days = Empty the recycle bin after (days)
admin-trash-days-hint = Posts older than this in the bin are deleted for good, hourly. 0 keeps them until emptied by hand.
admin-auto-tags = Tag new posts with what the file shows
admin-auto-tags-hint = Meta tags like highres, lowres, wide_image, video, sound and long_video. Each post is tagged once, so a tag you remove stays removed. Turning this on also tags older posts at the next server start.
admin-save = Save
admin-saved = Saved
admin-ai-title = AI
admin-ai-lead = Tag suggestions and translation of the text in images. Each can run on a hosted provider, on local AI on this server, or be off. Hosted providers receive thumbnails, and the text of an image when it is translated; local AI keeps everything on this server.
admin-ai-tagging = Tag suggestions
admin-ai-tagging-hint = Reads a thumbnail and suggests tags and a rating.
admin-ai-translation = Translation
admin-ai-translation-hint = Translates the text found in images.
admin-ai-route-off = Off
admin-ai-route-hosted = Hosted
admin-ai-route-local = Local
admin-ai-hosted = Hosted provider
admin-ai-hosted-lead = An OpenAI-compatible API: OpenAI, OpenRouter, Gemini, or LM Studio and vLLM on your network.
admin-ai-local = Local AI
admin-ai-local-lead = Runs a model on this server's own hardware through Ollama. Nothing leaves the server, and there is nothing to pay per image.
admin-ai-local-on = Use local AI
admin-ai-local-url = Ollama address
admin-ai-local-url-hint = http://ollama:11434 with the Docker Compose setup, or http://127.0.0.1:11434 when Ollama runs on this machine.
admin-ai-local-model = Local model
admin-ai-hw = This server:
admin-ai-hw-threads = { $count } threads
admin-ai-hw-memory = { $total } memory ({ $free } free)
admin-ai-hw-igpu = { $name } integrated graphics
admin-ai-hw-no-gpu = no graphics card found
admin-ai-runtime-ok = Ollama { $version } is running.
admin-ai-runtime-down = Ollama doesn't answer: { $error }. Start it, check the address and save.
admin-ai-tier-small = Small
admin-ai-tier-small-hint = For a Raspberry Pi 5 or another small board. Slow, but fine for tagging in the background.
admin-ai-tier-medium = Medium
admin-ai-tier-medium-hint = For a laptop or mini PC with integrated graphics and 8 GB of memory or more.
admin-ai-tier-large = Large
admin-ai-tier-large-hint = For a desktop with a graphics card of 8 GB or more. The best tags and translations.
admin-ai-tier-custom = Another model
admin-ai-tier-custom-hint = Any Ollama model that reads images, such as gemma3:4b.
admin-ai-recommended = Recommended
admin-ai-fit-good = Runs well
admin-ai-fit-slow = Runs slowly
admin-ai-fit-poor = Too slow
admin-ai-fit-nofit = Won't fit
admin-ai-fit-unknown = Can't tell
admin-ai-runs-gpu = on the graphics card, needs { $need }
admin-ai-runs-partly-gpu = partly on the graphics card, needs { $need }
admin-ai-runs-shared-gpu = on the integrated graphics, needs { $need } of memory
admin-ai-runs-cpu = on the processor, needs { $need } of memory
admin-ai-runs-none = needs { $need } of memory, more than this server has
admin-ai-installed = Downloaded
admin-ai-download = Download
admin-ai-downloading = Downloading { $model }: { $status }
admin-ai-download-failed = Downloading { $model } failed: { $error }
admin-ai-not-downloaded = The chosen model isn't downloaded yet. Download it before you use it.
admin-ai-test = Test the speed
admin-ai-testing = Testing…
admin-ai-test-result = Tagged a test image in { $seconds } s.
admin-ai-test-result-gpu = Tagged a test image in { $seconds } s, { $gpu }% on the graphics card.
admin-ai-test-hint = Tests the saved model on a small generated image. The first run also loads the model, so run it twice.
admin-ai-url = API address
admin-ai-url-hint = The base address, ending in /v1. A server on your own network keeps images private.
admin-ai-model = Model
admin-ai-key = API key
admin-ai-key-set = A key is set; type to replace it
admin-ai-key-none = No key
admin-ai-key-hint = Kept on the server and never shown again. Local servers usually need none.
admin-ai-key-clear = Remove the key
admin-ai-autotag = Tag new uploads automatically
admin-ai-autotag-hint = After each upload, the suggested tags are added in the background.
ai-suggest = Suggest tags
ai-suggesting = Asking the AI…
ai-add-all = Add all
ai-new-tag = new
ai-no-suggestions = No suggestions for this post.
ai-rating = Suggested rating: { $rating }
settings-admin-note = Server stats, accounts and settings
settings-storage = Uploads
usage-of = { $used } of { $quota }

## Backups

admin-tab-backups = Backups
backups-title = Database backups
backups-lead = A backup holds the database: posts, tags, accounts and settings. Media files stay in the data folder, so copy that folder as well.
backups-unavailable = This server can't make backups: the PostgreSQL client (pg_dump) is missing. Install it, or point STARBOORU_PG_DUMP at it.
backups-now = Back up now
backups-running = Backing up…
backups-none = No backups yet.
backups-automatic = Automatic
backups-download = Download
backups-delete = Delete backup
backups-delete-named = Delete backup { $name }
backups-schedule-title = Automatic backups
backups-every = Every (days)
backups-every-hint = 0 turns automatic backups off.
backups-keep = Keep
backups-keep-hint = Older automatic backups are removed. Backups made by hand stay until you delete them.
backups-restore-title = Restoring
backups-restore = Stop StarBooru, then load the backup into its database with pg_restore:
backups-restore-note = Start StarBooru again afterwards. The deployment guide has the full steps.

## Provider sign-in

login-or = or
login-with = Sign in with { $provider }
oidc-waiting = Finish signing in in your browser, then come back here.
oidc-finishing = Signing you in…
oidc-denied = The sign-in was cancelled.
oidc-failed = The sign-in didn't work. Try again, or ask the administrator.
oidc-expired = This sign-in has expired. Start again.
oidc-no-browser = Couldn't open a browser: { $error }
oidc-close-tab = Done. You can close this tab and go back to StarBooru.
oidc-back-settings = Back to Settings
oidc-back-sign-in = Back to sign-in
signin-title = Ways to sign in
signin-password-set = Set
signin-password-none = Not set
signin-provider = Single sign-on
signin-times = Linked { $linked } · last used { $used }
signin-linked = Linked { $linked }
signin-last = This is your only way to sign in. Set a password before unlinking it.
signin-link = Link a { $provider } account
signin-unlink = Unlink
signin-unlink-named = Unlink { $name }
password-none = This account has no password yet. Set one to sign in without the provider too.
password-set = Set password

## Share links

share-button = Share
share-title =
    { $count ->
        [one] Share this post
       *[other] Share { $count } posts
    }
share-lead = Anyone with the link can see these posts without an account. Tags, votes and details about you stay hidden.
share-name = Title (optional)
share-name-placeholder = e.g. Holiday photos
share-lifetime = Link works for
share-until-removed = Until removed
share-create = Create link
share-ready = Link ready. Anyone who has it can open it.
share-link = Share link
share-copy = Copy link
share-done = Done
share-close = Close
share-posts =
    { $count ->
        [one] 1 post
       *[other] { $count } posts
    }
shares-title = Shared links
shares-lead = Links you made with Share. Removing one stops it working; a file someone already opened can keep loading for a few hours.
shares-none = No links yet. Use Share on a post, or select posts in the gallery.
shares-untitled = Untitled link
shares-made = Made { $when }
shares-views =
    { $count ->
        [0] Never opened
        [one] Opened once
       *[other] Opened { $count } times
    }
shares-delete = Remove link
shares-delete-named = Remove the link { $name }
shared-missing-title = This link doesn't work
shared-missing-body = It may have expired, or the person who shared it removed it.
shared-from = Shared from { $server }
shared-until = available until { $date }
shared-empty = Nothing here any more.
shared-open = Item { $n }
shared-previous = Previous
shared-next = Next

## Media editor

editor-open = Edit image or video
editor-title-image = Edit image
editor-title-video = Trim video
editor-tools = Image tools
editor-turn-left = Turn left
editor-turn-right = Turn right
editor-flip-h = Flip left to right
editor-flip-v = Flip top to bottom
editor-reset = Undo all changes
editor-crop-hint = Drag the corners to crop, or drag inside the box to move it.
editor-start = Start (seconds)
editor-end = End (seconds)
editor-now = Use current time
editor-now-title = Take the time the video is at now
editor-keeps = Keeps { $length } of { $total }.
editor-keyframe-hint = The original quality is kept where the video allows it; otherwise the clip is re-encoded, which can take a while.
editor-bad-time = Enter a start before the end, within the video.
editor-too-short = The clip must be at least a tenth of a second long.
editor-no-change = Make a change first.
editor-bin-original = Move the original to the recycle bin
editor-copy-hint = The result is saved as a new post with the same tags, rating and source.
editor-save = Save as new post
editor-saving = Saving…

## Smaller copies

post-sample-note = Showing a smaller copy. Zooming in loads the original.
post-show-original = Show original

## Text in images (OCR)

post-ocr-title = Text in image
post-ocr-copy = Copy
post-ocr-copied = Copied
post-ocr-more = Show all
post-ocr-less = Show less
post-ocr-translate = Translate
post-ocr-translating = Translating…
post-ocr-original = Show original
post-ocr-machine = Translated by AI; it may contain mistakes.
settings-originals = Always open originals in the viewer
settings-originals-hint = Big images otherwise open as a smaller copy, which loads faster and uses less data. The original still opens when you zoom in, or when the screen has more pixels than the copy.

## Media cache (desktop and Android)

cache-title = Storage on this device
cache-hint = Thumbnails and the images you open are kept on this device, so they show at once next time.
cache-used = Images kept: { $size }
cache-limit = Keep up to
cache-off = Nothing (off)
cache-clear = Clear

## Downloads (desktop)

notice-close = Close
download-saved = Saved to { $path }
download-failed = Download failed: { $reason }
download-no-folder = no Downloads folder found

## Hand (tablet rail, viewer panel and tools)

settings-hand = Hand
settings-hand-hint = Puts the side navigation on tablets, and the viewer's panel and zoom tools, on the side of the hand you use.
hand-either = Either
hand-left = Left
hand-right = Right

## Battery saver

settings-saver = Battery saver
settings-saver-hint = While it's on, videos don't start by themselves, the next post isn't loaded ahead, and animations are cut short. Automatic turns it on when the battery is at 20% or less and not charging, or when the device is saving data.
settings-saver-now = On now: the battery is low or the device is saving data.
saver-auto = Automatic
saver-always = Always
saver-never = Never

## Wallpaper colour

accent-system = Wallpaper
accent-system-hint = Follow the colours Android takes from your wallpaper

## Other boorus
boorus-open = Other boorus
boorus-title = Other boorus
boorus-hint = Search Danbooru, Gelbooru, Moebooru, e621 and Blombooru sites, pick posts and import them with their tags, rating and source.
boorus-site = Site
boorus-custom = Another site…
boorus-address = Address
boorus-api = Kind of site
boorus-account = Account (optional)
boorus-account-hint = Some sites only answer, or allow longer searches, with your account. It's kept on this device and sent only to that site, through your server.
boorus-login = Username
boorus-user-id = User ID
boorus-key = API key
boorus-password-hash = Password hash
boorus-search = Search
boorus-search-label = Search on that site
boorus-search-placeholder = Tags, in that site's syntax
boorus-nothing = Nothing found on that site.
boorus-more = Load more
boorus-post = Post { $id }
boorus-known = Already here
boorus-extra-tags = Extra tags for these posts
boorus-import-label = Import the selected posts
boorus-import = Import { $count }
boorus-some-failed = Some posts weren't imported: { $error }
