// Write text to the clipboard, failing silently when the API is unavailable
// (insecure context, denied permission, etc.). Returns nothing; callers that
// need to react to success/failure should not use this thin helper.
export async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text)
  } catch {
    /* clipboard may be unavailable; fail silently */
  }
}
