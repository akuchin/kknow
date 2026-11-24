import type {
  Review,
} from "../api";
import { coded, find, proposals, threads } from "./store";
import { view } from "./views";
import type { Handlers } from "./handlers";

export const proposalsHandlers = {
  review_proposal: (args) => {
    const proposal = proposals.find((each) => each.id === args.proposal);
    if (!proposal) throw coded("unknownProposal", `proposal ${String(args.proposal)} not found`, { value: String(args.proposal) });
    const note = find(proposal.note);
    const review = args.review as Review;
    let content = note.content;
    for (const index of [...review.accept].sort((a, b) => a - b)) {
      const edit = proposal.edits[index];
      if (content.split(edit.old).length !== 2) {
        throw coded("staleEdit", `in proposal ${proposal.id}, change ${index + 1} no longer finds its text: the note has changed`, { value: proposal.id, line: String(index + 1) });
      }
      content = content.replace(edit.old, edit.new);
    }
    note.content = content;
    for (const index of review.accept) proposal.edits[index].state = "accepted";
    for (const index of review.reject) proposal.edits[index].state = "rejected";
    if (review.feedback) proposal.feedback = review.feedback;
    return view(note);
  },

  dismiss_proposal: (args) => {
    const index = proposals.findIndex((each) => each.id === args.proposal);
    if (index >= 0) proposals.splice(index, 1);
    return;
  },

  reply_thread: (args) => {
    const note = find(args.note);
    let thread = threads.find((each) => each.id === args.thread);
    if (!thread) {
      const id = `t${threads.length + 1}`;
      thread = {
        id,
        note: note.id,
        anchor: "",
        task: String(args.text ?? ""),
        status: "open",
        messages: [{ from: "me", text: String(args.text ?? ""), at: Date.now() / 1000 }],
        created: Date.now() / 1000,
        updated: Date.now() / 1000,
      };
      threads.push(thread);
      const lines = note.content.split("\n");
      const number = (args.line ?? 1) - 1;
      lines[number] = (lines[number] ?? "").replace("<!-- @agent ", `<!-- @agent#${id} `).replace("<!-- @agent:", `<!-- @agent#${id}:`);
      note.content = lines.join("\n");
    }
    thread.messages.push({ from: "me", text: args.message, at: Date.now() / 1000 });
    if (thread.status === "question" || thread.status === "inProgress") thread.status = "answered";
    thread.updated = Date.now() / 1000;
    return view(note);
  },
} satisfies Handlers;
