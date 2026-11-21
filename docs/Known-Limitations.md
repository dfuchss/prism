# Known limitations

- **The builds are not signed.** macOS Gatekeeper and Windows SmartScreen warn on the first start; see [macOS](macOS#install) and [Windows](Windows#install).
- **No updater.** PRism does not update itself: download a newer release to update.
- **GitHub.com only.** There is no GitHub Enterprise Server support.
- **Approvals and merges cannot be undone from the app.** That is why they are [bound to the head commit](Rules#safety-model) and automatic rules are restricted.
- **The merge method is the repository's choice.** `merge` without a method uses the first method the repository allows.

The limitations that come from one platform are listed on its page: [macOS](macOS#known-limitations), [Linux](Linux#known-limitations), [Windows](Windows#known-limitations).
