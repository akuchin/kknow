// Agent proposals: edits that an agent suggested through MCP (`propose_edit`).
//
// The note does not change until the person accepts. The float lists the
// waiting proposals of the open note; each edit shows what goes away and
// what comes instead, with a box to keep or drop it. "Apply" sends the
// decisions to the core, which writes the accepted edits and keeps the
// rejected ones as an answer for the agent (`proposal_status`).

import { api, type NoteView, type ProposalEdit, type ProposalView } from "../api";
import { bindDraft, bindDraftCheck, clearDraftsUnder, keepFocus } from "../drafts";
import { t } from "../i18n";
import { splitChange } from "../word-diff";
import { el } from "./dom";
import type { NoteCalls } from "./notes-float";
import { state } from "./state";
import { fail, setStatus } from "./status";

let calls: NoteCalls;

export function initProposalsFloat(noteCalls: NoteCalls): void {
  calls = noteCalls;
  el.toggleProposals.onclick = () => showProposalsFloat(el.proposalsFloat.hidden);
  el.proposalsHide.onclick = () => showProposalsFloat(false);
  state.on("note", (note) => showProposals(note));
}

function waiting(note: NoteView | null): ProposalView[] {
  return note?.file ? [] : (note?.proposals ?? []);
}

/**
 * The button shows only when the note has waiting proposals, as the comments
 * button does. An open float stays open after the last review: the person
 * sees "no proposals", not a window that vanished under the cursor.
 */
function showProposals(note: NoteView | null): void {
  const list = waiting(note);
  el.proposalsCount.textContent = String(list.length);
  el.toggleProposals.hidden = list.length === 0 && el.proposalsFloat.hidden;
  if (!el.proposalsFloat.hidden) renderProposals();
}

export function showProposalsFloat(show: boolean): void {
  el.proposalsFloat.hidden = !show;
  el.toggleProposals.setAttribute("aria-pressed", String(show));
  el.toggleProposals.hidden = !show && waiting(state.note).length === 0;
  if (show) renderProposals();
}

/**
 * Draws the list again. The feedback text and the boxes of each proposal are
 * drafts: a review of one proposal or a new view of the note does not reset
 * what the person typed or unchecked in another one.
 */
function renderProposals(): void {
  const refocus = keepFocus(el.proposalsList);
  try {
    fillProposals();
  } finally {
    refocus();
  }
}

function draftPrefix(proposal: ProposalView): string {
  return `proposal:${proposal.id}:`;
}

function fillProposals(): void {
  el.proposalsList.replaceChildren();
  const list = waiting(state.note);
  if (list.length === 0) {
    const empty = document.createElement("li");
    empty.className = "empty";
    empty.textContent = t("panel.proposals.empty");
    el.proposalsList.append(empty);
    return;
  }
  for (const proposal of list) el.proposalsList.append(proposalItem(proposal));
}

function diffOf(edit: ProposalEdit, create: boolean): HTMLElement {
  const box = document.createElement("div");
  box.className = "proposal-diff";
  if (create) {
    const added = document.createElement("ins");
    added.textContent = edit.new;
    box.append(added);
    return box;
  }
  const [before, removed, added, after] = splitChange(edit.old, edit.new);
  const del = document.createElement("del");
  del.textContent = removed;
  const ins = document.createElement("ins");
  ins.textContent = added;
  box.append(before, del, ins, after);
  return box;
}

function proposalItem(proposal: ProposalView): HTMLLIElement {
  const item = document.createElement("li");
  item.className = "proposal";

  const head = document.createElement("div");
  head.className = "proposal-head";
  const author = document.createElement("span");
  author.className = "proposal-author";
  author.textContent = proposal.author ?? t("panel.proposals.agent");
  const reason = document.createElement("span");
  reason.className = "proposal-reason";
  reason.textContent = proposal.create
    ? `${t("panel.proposals.newNote")}: ${proposal.reason}`
    : proposal.reason;
  head.append(author, reason);

  const edits = document.createElement("ol");
  edits.className = "proposal-edits";
  const boxes: (HTMLInputElement | null)[] = [];
  proposal.edits.forEach((edit, index) => {
    if (edit.state !== "pending") {
      boxes.push(null);
      return;
    }
    const row = document.createElement("li");
    row.className = "proposal-edit";
    const stale = proposal.stale[index] ?? false;
    const label = document.createElement("label");
    const box = document.createElement("input");
    box.type = "checkbox";
    box.disabled = stale;
    if (stale) box.checked = false;
    else bindDraftCheck(box, `${draftPrefix(proposal)}edit:${index}`, true);
    label.append(box, t("panel.proposals.accept"));
    if (stale) {
      row.classList.add("is-stale");
      label.title = t("panel.proposals.stale");
    }
    row.append(diffOf(edit, proposal.create), label);
    edits.append(row);
    boxes.push(box);
  });

  const feedback = document.createElement("textarea");
  feedback.className = "proposal-feedback";
  feedback.rows = 2;
  feedback.placeholder = t("panel.proposals.feedback");
  bindDraft(feedback, `${draftPrefix(proposal)}feedback`);

  const actions = document.createElement("div");
  actions.className = "proposal-actions";
  const apply = document.createElement("button");
  apply.className = "primary";
  apply.textContent = t("panel.proposals.apply");
  const reject = document.createElement("button");
  reject.textContent = t("panel.proposals.rejectAll");
  const dismiss = document.createElement("button");
  dismiss.className = "ghost";
  dismiss.textContent = t("panel.proposals.dismiss");
  dismiss.title = t("panel.proposals.dismiss.title");
  actions.append(apply, reject, dismiss);

  const decide = (acceptChecked: boolean) => {
    const accept: number[] = [];
    const rejected: number[] = [];
    boxes.forEach((box, index) => {
      if (!box) return;
      (acceptChecked && box.checked ? accept : rejected).push(index);
    });
    const text = feedback.value.trim();
    void review(proposal, { accept, reject: rejected, ...(text ? { feedback: text } : {}) });
  };
  apply.onclick = () => decide(true);
  reject.onclick = () => decide(false);
  dismiss.onclick = () => void drop(proposal);

  item.append(head, edits, feedback, actions);
  return item;
}

/**
 * Sends the decisions. The pending edits of the editor go to disk first: the
 * core searches the text of each edit there.
 */
async function review(
  proposal: ProposalView,
  decisions: { accept: number[]; reject: number[]; feedback?: string },
): Promise<void> {
  const id = state.note?.id;
  await calls.flush();
  try {
    const view = await api.reviewProposal(proposal.id, decisions);
    clearDraftsUnder(draftPrefix(proposal));
    if (state.note?.id !== id) return;
    // The text changed on disk: the editor takes the new text.
    calls.apply(view, { reload: true });
    setStatus(
      t(decisions.accept.length > 0 ? "status.proposalApplied" : "status.proposalRejected"),
    );
  } catch (error) {
    fail(error);
  }
}

async function drop(proposal: ProposalView): Promise<void> {
  const id = state.note?.id;
  if (!id) return;
  try {
    await api.dismissProposal(proposal.id);
    clearDraftsUnder(draftPrefix(proposal));
    if (state.note?.id !== id) return;
    calls.apply(await api.loadNote(id), { keepEditor: true });
  } catch (error) {
    fail(error);
  }
}
