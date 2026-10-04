// Queue responses to app dialogs without relying on browser-native dialogs.
const sessions = new WeakMap();
export function answerDialog(page, handler) {
  let state = sessions.get(page);
  if (!state) {
    state = { tail: Promise.resolve(), last: "", errors: [] };
    sessions.set(page, state);
    page.on("framenavigated", (frame) => {
      if (frame === page.mainFrame()) state.last = "";
    });
  }
  state.tail = state.tail
    .then(async () => {
      await page.waitForFunction((last) => {
        const dialog = document.querySelector("#actionDialog[open]");
        return dialog && dialog.dataset.request !== last;
      }, state.last);
      const dialog = page.locator("#actionDialog");
      state.last = await dialog.getAttribute("data-request");
      const message = await dialog
        .locator("#actionDialogMessage")
        .textContent();
      await handler({
        message: () => message,
        accept: async (value) => {
          if (value !== undefined)
            await dialog.locator("#actionDialogInput").fill(value);
          await dialog.locator("#actionDialogConfirm").click();
        },
        dismiss: () => dialog.locator("#actionDialogCancel").click(),
      });
    })
    .catch((error) => {
      state.errors.push(error);
    });
}
export async function finishDialogs(page) {
  const state = sessions.get(page);
  if (!state) return;
  await state.tail;
  if (state.errors.length) throw state.errors.shift();
}
