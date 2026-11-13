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
nav-boorus = Boorus
nav-tags = Tags
nav-upload = Upload
nav-settings = Settings
nav-settings-tab = Settings
search-placeholder = Search tags, e.g. cat rating:safe
demo-tag = Posts with this tag
demo-tag-all = Posts with every tag listed
demo-tag-not = A minus leaves a tag out
demo-figure = One person has all of these
demo-rating = Only this rating
demo-rating-any = Any of these ratings
demo-rating-not = Any rating but explicit
demo-type = Only videos
demo-date = From March 2024
demo-date-range = From 2023 to the end of 2024
demo-date-age = Less than 7 days old
demo-size = Wider than 1920 pixels
demo-ratio = Width over height is 16:9
demo-color = Has this colour
demo-text = This word is written in the picture
demo-order-score = Highest score first
demo-order-random = A new shuffle every time
demo-combined = Put them all together
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
login-weak-password = This password is easy to guess. Try a longer one, or a few words that don't go together.

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
settings-accent-custom = Custom accent colour
accent-no-hue = Greys have no hue to build the colours from. Pick a stronger colour.
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
about-title = About StarBooru
about-made-by = Made by
about-icons = Icons
about-lettering = Logo lettering
about-version = App version
about-credits = All credits
about-credits-note = Everyone whose work StarBooru uses, with their licenses
password-current = Current password
password-new = New password
password-too-short = At least { $count } characters.
password-repeat = Repeat the new password
password-mismatch = The passwords don't match.
password-changed = Password changed. Your other devices were signed out.
password-changing = Changing…
password-change = Change password
password-strength-weak = Easy to guess
password-strength-fair = Could be stronger
password-strength-strong = Strong password
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

rating-safe = safe
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
help-tag-tree = has footwear or any tag under it, like boots or sneakers
help-figure = one person in the picture has all of these tags; girl, boy or other says who. A minus in front finds posts where no one does
help-tag-category = the tag some_name; when tagging, the prefix makes it an artist tag (also character:, copyright: and meta:)
help-tag-spaces = two tags: spaces separate terms, so write blue_sky
help-tag-meta = high-resolution posts without sound. The server adds meta tags like lowres, highres, absurdres, wide_image, tall_image, hd, full_hd, 4k, 8k, horizontal_image, vertical_image, square_image, animated, video, sound and long_video by itself
help-filters = Filters
help-rating = rated safe, sensitive (s), questionable (q) or explicit (e)
help-rating-any = any of the listed ratings
help-rating-not = any rating but explicit
help-type = still images; also type:animation and type:video
help-deleted = posts in the recycle bin
help-date = the date in the file, or the upload date if it has none; also date:2024 and date:2024-03-15
help-date-range = from the start of the first period to the end of the last; date:2023.. and date:..2024 leave one end open
help-date-compare = after that period; also >=, < and <=
help-date-age = more than 30 days old; date:<7d is less than 7 days old. Also h, w, mo and y
help-size = wider than 1920 pixels; also height:, >=, <, <=, an exact width:1920 and a range width:1000..2000
help-mpixels = more than 8 megapixels; a bare mpixels:8 is about 8
help-ratio = width over height, as 16:9 or 1.78, within 0.01; ratio:1 is square and ratio:>1 is wider than tall
help-size-not = any size but these; a post of unknown size stays in
help-color = has this colour or, when no post does, the nearest one stored; also orange, yellow, green, teal, blue, purple, pink, brown, black, gray and white
help-color-hex = any colour, in hex; also #2060e0 and 26e
help-color-not = doesn't have this colour
help-exact = an unknown tag is normally corrected to the closest one in use; this searches tags exactly as typed
help-text = Text in images
help-text-word = the text read in the image has the word (any case)
help-text-phrase = has all these words; quotes keep them together
help-text-not = the text read in the image doesn't have the word
help-sorting = Sorting
help-order-new = newest first (the default); order:old for oldest
help-order-score = highest score first
help-order-size = largest file first
help-order-pixels = highest resolution first; order:mpixels too
help-order-longside = longest side first
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
help-shortcuts = Keyboard shortcuts and gestures
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
key-swipe = Swipe left or right
key-swipe-action = previous or next post, with a finger or two on a touchpad
key-swipe-down = Swipe down
key-swipe-down-action = leave full screen, then the post
key-tap = Tap the picture
key-tap-action = full screen
key-hold = Hold, or right click
key-hold-action = a menu for a thumbnail or a tag
key-swipe-tile = Swipe a thumbnail
key-swipe-tile-action = what you chose in Settings › Gestures
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
explain-figure = One person has all of { $tags }
explain-no-figure = No one has all of { $tags }
explain-figure-kind = A person ({ $kind }) has all of { $tags }
explain-no-figure-kind = No person ({ $kind }) has all of { $tags }
explain-figure-only = A person marked as { $kind }
explain-no-figure-only = No person marked as { $kind }
explain-date = Dated { $value }
explain-not-date = Not dated { $value }
explain-size = { $field } { $value }
explain-not-size = { $field } not { $value }
explain-color = Has the colour { $value }, or the nearest one stored
explain-not-color = Doesn't have the colour { $value }
explain-exact = Tags are taken exactly as typed
explain-size-width = Width
explain-size-height = Height
explain-size-mpixels = Megapixels
explain-size-ratio = Aspect ratio
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
explain-order-longside-desc = By longest side, longest first
explain-order-longside-asc = By longest side, shortest first
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
privacy-13 = The server only contacts other sites when you import from a URL or from another booru, and the operator’s identity provider when you sign in through it. Imports carry a generic user agent and none of your details. If an administrator sets up an AI provider, thumbnails of posts are sent to it to suggest tags, the text read in an image when you translate it, and the image itself when its comic or manga panels are translated; a provider on the operator’s own network keeps them there. While a whole library is being imported, the key you gave for that site is kept on the server, and in any backup taken meanwhile; it’s erased when the import ends.
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
upload-drop = Drop files or links
upload-files = Choose files
upload-link = Add a link
upload-drop-now = Drop to upload
upload-folder = Choose a folder
upload-folder-tags = Tag files with their folder name
upload-urls-placeholder = Paste a link to a file or a booru post
upload-urls-label = Links to files or booru posts
add = Add
upload-shared-tags = Tags for all files
rating = Rating
upload-clear = Clear list
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
upload-ready = Up next
upload-remove = Remove { $name }
upload-done = Imported
upload-job-running = Importing
upload-job-cancel = Cancel
activity-signed-out = You're signed out.
activity-pending =
    { $count ->
        [one] 1 upload or import in progress
       *[other] { $count } uploads and imports in progress
    }
activity-uploaded =
    { $count ->
        [one] 1 file uploaded
       *[other] { $count } files uploaded
    }
activity-imported =
    { $count ->
        [one] 1 post imported
       *[other] { $count } posts imported
    }
activity-known = { $count } already here
activity-failed = { $count } failed

## Tags

category-general = General
category-artist = Artist
category-character = Character
category-copyright = Copyright
category-meta = Meta
section-people = People
section-hair = Hair
section-eyes = Eyes
section-skin = Skin
section-body = Body
section-expression = Expression
section-pose = Pose and framing
section-clothing = Clothing
section-setting = Setting
section-style = Style
section-other = Other
people-title = People
people-none = No one is marked yet. Adding the people in the picture tags their clothes and looks one by one, and keeps 1girl, 2boys and solo right.
people-hint = Their tags go on the post too. Counts like 1girl and solo follow the people. Separate tags with spaces.
people-add = Add a person
people-edit = Edit
people-remove = Remove this person
people-kind = Who
people-girl = Girl
people-boy = Boy
people-other = Other
people-numbered = { $kind } { $n }
people-character = Character
people-cosplay = Cosplaying
people-as = as
people-clothing = Clothing
people-looks = Looks
slot-head = Head
slot-body = Body
slot-bottom = Bottom and legs
slot-shoes = Shoes
slot-wear = Other clothing
slot-shape = Build
slot-eyes = Eyes
slot-skin = Skin
slot-hair = Hair
slot-pose = Pose
slot-other = Other
tags-tree = Place in the tree
tags-tree-hint = Searching a tag also finds the tags under it, and the top of the tree is the section a post's tags are grouped under.
tags-tree-top = At the top: nothing above it.
tags-parent = Parent tag
tags-move = Move under
tags-move-top = Move to the top
tags-children = Under it
tags-children-none = Nothing under it yet.
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
post-actions = Post actions
post-back = Back
post-upvote = Upvote
post-downvote = Downvote
post-download = Download
post-edit = Edit
post-edit-tags = Edit tags
post-delete = Delete
post-delete-title = Move to recycle bin
post-tags = Tags
post-tags-hint = Prefix a tag with artist:, character:, copyright: or meta: to set its category.
post-did-you-mean = Did you mean?
post-replace-tag = Replace { $from } with { $to }
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
post-file-date = File date
post-colors = Colours
post-color-search = Search for { $color }
post-source = Source
post-prev-title = Previous (←)
post-prev = Previous post
post-next-title = Next (→)
post-next = Next post
zoom-out-title = Zoom out (−)
zoom-out = Zoom out
zoom-fit = Fit
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
view-title = Gallery layout
view-shape = Tile shape
view-square = Square
view-landscape = Landscape
view-portrait = Portrait
view-staggered = Staggered
view-staggered-by = Staggered shapes
view-everything = Everything
view-columns = { $count ->
    [one] 1 column
   *[other] { $count } columns
}
view-per-row = About { $count } per row
view-columns-auto = Automatic
gallery-stop-selecting = Stop selecting
gallery-select = Select posts
gallery-syntax = Search syntax
gallery-to-top = Back to the top
search-corrected = Spelling corrected: { $tags }
search-exact = Search exactly as typed
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
color-search = Search by colour
color-wheel = Pick a colour from the wheel
color-hex = Hex colour
color-hex-apply = Use this colour
color-hex-invalid = Use 3 or 6 hex digits, like #2060e0
color-clear = Any colour
color-red = Red
color-orange = Orange
color-yellow = Yellow
color-green = Green
color-teal = Teal
color-blue = Blue
color-purple = Purple
color-pink = Pink
color-brown = Brown
color-black = Black
color-gray = Gray
color-white = White
sort-new = Newest
sort-old = Oldest
sort-score = Top score
sort-updated = Recently edited
sort-size = Largest file
sort-size-asc = Smallest file
sort-pixels = Highest resolution
sort-longside = Longest side
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
admin-ai-unavailable = Local AI can't start: { $reason }
admin-ai-hw-threads = { $count } threads
admin-ai-hw-memory = { $total } RAM
admin-ai-tagging = Tag suggestions
admin-ai-translation = Translation
admin-ai-comics = Comic translation
admin-ai-mode-off = Off
admin-ai-mode-local = Local
admin-ai-mode-api = API
admin-ai-speed = Speed
admin-ai-preset-fast = Fast
admin-ai-preset-balanced = Balanced
admin-ai-preset-accurate = Accurate
admin-ai-presets-off = Not available on this machine: { $presets }
admin-ai-state-ready = Ready
admin-ai-state-preparing = Preparing…
admin-ai-state-downloading = Downloading { $percent }%
admin-ai-state-failed = Failed
admin-ai-retry = Retry
admin-ai-models = Models
admin-ai-no-memory = Not enough memory
admin-ai-gpu = GPU
admin-ai-high-accuracy = High accuracy
admin-ai-in-use = In use
admin-ai-use = Use
admin-ai-install = Install
admin-ai-uninstall = Uninstall
admin-ai-url = Address
admin-ai-model = Model
admin-ai-key = API key
admin-ai-key-set = Saved
admin-ai-key-none = None
admin-ai-key-clear = Remove key
admin-ai-test = Test
admin-ai-testing = Testing…
admin-ai-test-ok = Answered in { $time }
admin-ai-autotag = Tag new uploads automatically
admin-ai-auto-translate = Translate new comics to
admin-ai-auto-translate-hint = Language code, such as en. Empty: don't.
ai-suggest = Suggest tags
ai-suggesting = Asking the AI…
ai-add-all = Add all
ai-new-tag = new
ai-no-suggestions = No suggestions for this post.
ai-suggest-people = Suggest people
ai-no-people = The AI found no people.
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
editor-image-short = Edit image
editor-video-short = Trim
editor-title-image = Edit image
editor-title-video = Trim video
editor-tools = Tools
editor-turn-left = Turn left
editor-turn-right = Turn right
editor-flip-h = Flip left to right
editor-flip-v = Flip top to bottom
editor-crop = Crop
editor-trim = Trim
editor-history = History
editor-undo = Undo
editor-redo = Redo
editor-undo-keys = Undo (Ctrl+Z)
editor-redo-keys = Redo (Ctrl+Shift+Z)
editor-save-keys = Save as a new post (Ctrl+S)
editor-ratio = Proportions
editor-ratio-free = Free
editor-ratio-original = Original
editor-ratio-custom = Custom
editor-ratio-swap = Portrait or landscape
editor-ratio-width = Width
editor-ratio-height = Height
editor-straighten = Straighten
editor-straighten-reset = Back to 0°
editor-turn-flip = Turn and flip
editor-keep-original = Keep original
editor-keep-original-hint = The edit is saved as a new post beside it
editor-saved = Saved as a new post
editor-saved-binned = Original moved to the recycle bin
editor-undo-save = Undo
editor-reset = Start over
editor-start = Start (seconds)
editor-end = End (seconds)
editor-now = Use current time
editor-now-title = Take the time the video is at now
editor-keeps = Keeps { $length } of { $total }.
editor-keyframe-hint = The original quality is kept where the video allows it; otherwise the clip is re-encoded, which can take a while.
editor-bad-time = Enter a start before the end, within the video.
editor-too-short = The clip must be at least a tenth of a second long.
editor-no-change = Make a change first.
editor-save = Save
editor-save-copy = Save copy
editor-save-options = What happens to the original
editor-save-replace = Replace original
editor-save-replace-hint = The original goes to the recycle bin

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

## Translated text over the image (comics and manga)

post-panels-make = Translate the image
post-panels-make-title = Translate the text in the image, over the picture. The image is sent to the server's translation AI.
post-panels-making = Translating the image…
post-panels-show = Show the translation
post-panels-hide = Hide the translation
post-panels-none = No text found to translate.
settings-originals = Always open originals in the viewer
settings-originals-hint = Big images otherwise open as a smaller copy, which loads faster and uses less data. The original still opens when you zoom in, or when the screen has more pixels than the copy.
settings-flush = Remove the spaces and rounded corners between images
settings-flush-hint = Thumbnails sit edge to edge, with square corners.

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
settings-hand-hint = Puts the side navigation on tablets and laptops, and the viewer's panel and zoom tools, on the side of the hand you use.
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

## Effects

settings-effects = Effects
settings-effects-hint = Reduced shows plain surfaces in place of the frosted glass, and leaves out the entrance of new thumbnails and the glow behind a picture. Automatic does that once scrolling stutters on this device.
settings-effects-now = Reduced now: scrolling was stuttering on this device. Choose Automatic again to measure it once more.
effects-auto = Automatic
effects-full = Full
effects-lite = Reduced

## Wallpaper colour

accent-system = Wallpaper
accent-system-hint = Follow the colours Android takes from your wallpaper

## Booru browser
boorus-search = Search boorus
boorus-sites = Sites
boorus-site = Site
boorus-site-remove = Remove { $site }
boorus-address = Address
boorus-api = Kind of site
boorus-account = Account
boorus-account-forget = Forget
boorus-account-again = Enter it again
boorus-login = Username
boorus-user-id = User ID
boorus-key = API key
boorus-password-hash = Password hash
boorus-dedupe = Hide duplicates
boorus-known = Already here
boorus-extra-tags = Extra tags
boorus-save-label = Save the selected posts
boorus-save = Save { $count }
boorus-open-site = Open on site
boorus-also = Also on
boorus-score = Score

library-title = Import from another booru
library-lead = Moves a whole library here from szurubooru, Shimmie2, Danbooru, Stash, Blombooru, Monbooru or LANBooru: every post with its tags, rating, source, date and your likes, plus aliases, implications, tag trees, and pools or albums as tags. Your server does the work, so you can close this app. Importing again adds what’s new, and a post you already have only gains tags. Aliases and implications come only when an administrator imports. Your key stays on the server while the import runs and is erased when it ends.
library-left-out = Stays behind: comments, other users and their accounts, notes on images, titles and descriptions, site-wide scores and the order inside pools. StarBooru has no place for them.
library-source = Import from
library-hint-szurubooru = Log in with your username and a login token from your account’s Login tokens tab. Without them, you get what guests can see.
library-hint-shimmie2 = Needs the Ouroboros API turned on in Shimmie’s Extension Manager, and an API key if guests can’t see everything. Its aliases and auto-tag rules come across if they’re public, except those of several words. Its pools can’t be read: Shimmie has no API for them.
library-hint-danbooru = Your username and the API key from your profile. Without them, you get what guests can see, and your favourites stay behind. On large sites this means millions of posts, so use it on your own instance.
library-hint-stash = Scenes and images, oldest first. The API key is in Settings › Security. Stash has no safety ratings, so every post gets the rating you choose below. Performers become character tags, studios artist tags, and a rating of 60 or more becomes a like.
library-hint-blombooru = The API key from your account. Aliases and implications need an admin’s key on that side; tag-pattern implications don’t come across.
library-hint-monbooru = A token with read access, if the site uses them. Every gallery on the server comes across. Its aliases can’t be read: Monbooru has no API for them.
library-hint-lanbooru = No account needed: LANBooru’s reads are open. It has no aliases, implications or albums.
library-unrated = Rating for posts without a defining tag
library-check = Check
library-checking = Checking…
library-start = Import everything
library-found = Found { $count } posts.
library-found-some = The site answers, but doesn’t say how many posts it has.
library-no-rules = Aliases and implications will stay behind, because only an administrator can import them here.
library-full = Up to five imports can wait or run at once. Start this one when another ends.
library-bad-address = That address isn’t valid. Write it like http://192.168.1.4:8080.
library-need-login = This site needs the username along with the key.
library-need-key = This site needs the key along with the username.
library-list = Imports
library-state-queued = Waiting
library-state-rules = Reading aliases and implications
library-state-posts = Importing posts
library-state-groups = Reading pools and likes
library-state-done = Finished
library-state-stopped = Stopped
library-state-failed = Failed
library-count = { $done } done
library-count-of = { $done } of { $total }
library-count-merged = { $count } already here
library-count-failed = { $count } failed
library-count-rules = { $count } aliases and implications
library-stop = Stop
library-retry = Import again
library-remove = Remove from list
library-show-failed = Show failed
library-account-again = The key was erased when this import ended. Enter it again, or leave it empty to read what guests can see.

## Gestures and action menus
swipe-off = Nothing
swipe-like = Like
swipe-select = Select
swipe-delete = Move to recycle bin
press-open = Open
press-unlike = Remove like
press-liked = Liked
press-unliked = Like removed
press-deleted = Moved to the recycle bin
press-undo = Undo
settings-gestures = Gestures
settings-gestures-hint = Swipe a thumbnail sideways to act on it at once. Hold a thumbnail or a tag, or right click it, for all it can do.
settings-swipe-left = Swipe left
settings-swipe-right = Swipe right
tag-menu-search = Search this tag
tag-menu-add = Add to this search
tag-menu-exclude = Leave it out
tag-menu-details = Tag details
tag-menu-copy = Copy name
tag-copied = Tag name copied
