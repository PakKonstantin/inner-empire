# App Store readiness

## 1. Permissions — and the ones deliberately absent

§73 says not to request what is not needed. The full set:

| Key | Requested when | String |
|---|---|---|
| `NSPhotoLibraryUsageDescription` | the user taps "Insert from Photos" | "Inner Empire adds the photos you choose to your vault's attachments folder." |
| `NSCameraUsageDescription` | the user taps "Scan document" or "Take photo" | "Inner Empire uses the camera only when you take a photo or scan a document into a note." |

That is all. Explicitly **not** requested:

- location, contacts, calendar, reminders, microphone, health, Bluetooth,
  local network, tracking (`NSUserTrackingUsageDescription` is absent because
  there is no tracking), Face ID, Siri beyond the App Intents that need no
  authorization.

Both permissions are requested at the moment of use, never at launch, and both
features work without them in the sense that the rest of the app is unaffected.

## 2. Entitlements

| Entitlement | Why |
|---|---|
| App Group `group.<prefix>.innerempire` | the share extension hands captured items to the app |
| `com.apple.security.application-groups` | same |

Not present: iCloud container (the app uses the user's own folder through the
document picker, not a private container), push, background fetch, network
extensions, keychain sharing.

## 3. Info.plist document integration

```
UIFileSharingEnabled                    = YES
LSSupportsOpeningDocumentsInPlace       = YES
UISupportsDocumentBrowser               = NO
```

The first two make the vault reachable from Files.app. The third is off because
the app is folder-scoped, not document-browser-rooted.

`CFBundleDocumentTypes` declares `net.daringfireball.markdown` and
`public.plain-text` as viewer/editor roles, and a custom
`com.innerempire.canvas` type for `.canvas` files. `UTExportedTypeDeclarations`
declares the canvas type, conforming to `public.json`.

`CFBundleURLTypes` declares the `innerempire` scheme for deep links. The URL
parser rejects anything it does not recognise, resolves note references through
`VaultPath::parse` (so a link cannot escape the vault), and never performs a
write in response to a URL without a visible confirmation.

## 4. Background modes

`UIBackgroundModes = ["processing"]`, used only for `BGProcessingTaskRequest`
with identifier `<prefix>.innerempire.index`. Indexing is checkpointed per batch
so the system can stop it at any moment with no loss. No background fetch, no
audio, no location, no VoIP.

## 5. Privacy manifest

`PrivacyInfo.xcprivacy` declares:

- `NSPrivacyTracking` — `false`
- `NSPrivacyTrackingDomains` — empty
- `NSPrivacyCollectedDataTypes` — **empty**. The app collects nothing.
- `NSPrivacyAccessedAPITypes` — `NSPrivacyAccessedAPICategoryFileTimestamp`
  (reason `C617.1` — the app's own files, for change detection),
  `NSPrivacyAccessedAPICategoryDiskSpace` (reason `E174.1` — checking space
  before a write), `NSPrivacyAccessedAPICategoryUserDefaults` (reason
  `CA92.1` — the app's own settings and bookmark).

No third-party SDK is linked, so no third-party manifests need aggregating.

## 6. Export compliance

`ITSAppUsesNonExemptEncryption = false`. The app performs no encryption; SHA-256
is used for content hashing in the index, which is not encryption and is exempt.

## 7. Review notes to submit

The reviewer needs a vault to see anything, so the submission includes:

- a demo vault the reviewer can create with one tap from the empty state
  (the "Create a vault" path makes a folder with a welcome note, exactly as the
  desktop does)
- a note that the app works entirely offline and makes no network requests
- a note that the only "plugin" surface is declarative manifests interpreted by
  the app, with no downloaded or executed code — relevant to guideline 2.5.2

## 8. Assets

App icon at every required size, generated from one 1024pt source; a launch
screen that is a plain background with the app's surface colour so it does not
flash white in dark mode; no launch image assets.

## 9. What is knowingly not ready

Nothing in this document has been verified on Apple hardware. It is the checklist
to work through on a Mac, and the Release configuration exists to make that
possible — not evidence that submission would pass today.
