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
help-filters = Filters
help-rating = rated general (g), sensitive (s), questionable (q) or explicit (e)
help-rating-any = any of the listed ratings
help-rating-not = any rating but explicit
help-type = still images; also type:animation and type:video
help-deleted = posts in the recycle bin
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
privacy-5 = Your account: username, role and a salted Argon2id hash of your password. The password itself is never stored.
privacy-6 = Your sessions: a hash of each sign-in token, the kind of device it came from (for example “StarBooru app on Linux” or “Firefox on Android”), when it was created and last used, and when it expires. API keys you create are kept the same way: a hash of the key, its name, and when it was last used.
privacy-7 = What you add: uploaded files with their size, dimensions, checksum and a visual fingerprint for the duplicate finder, plus tags, ratings, sources, votes and saved searches.
privacy-8 = To slow down password guessing, the server counts failed sign-ins per account and per network address, and sign-ups per address. These counts are kept in memory for at most an hour and never written to disk.
privacy-9 = What stays on your device
privacy-10 = The app remembers the server address, your sign-in token and your preferences, such as the theme. Signing out removes the token.
privacy-11 = What StarBooru does not do
privacy-12 = No analytics, advertising or tracking, and no third-party services.
privacy-13 = The server only contacts other sites when you import from a URL. Those requests carry a generic user agent and none of your details.
privacy-14 = File contents, passwords and tokens are never written to the server's logs.
privacy-15 = Who can see your posts
privacy-16 = Everyone with an account on this server can see the posts on it. Media links handed to the app are signed and expire, so a copied link stops working within 12 hours.
privacy-17 = Keeping and deleting
privacy-18 = Deleted posts go to the recycle bin. They are erased when an administrator empties it, or automatically after the period the operator sets.
privacy-19 = Sessions end when you sign out, when they expire, or when you sign out other devices in Settings. Changing your password signs out every other device.
privacy-20 = To delete your account, ask an administrator of this server.
privacy-21 = The operator may keep backups, which follow their own schedule.
terms-1 = These terms cover your use of this StarBooru server. The operator who runs it may add to them.
terms-2 = Your account
terms-3 = Keep your password to yourself, and sign out devices you no longer use (Settings › Signed-in devices).
terms-4 = The first account on a server is its administrator. Administrators can manage tags, aliases and implications, and empty the recycle bin.
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
