// Text helpers every feature shares. Pure.

/** The core's lower-case clause (an error message, a refusal) as a sentence. */
export function sentence(message: string): string {
  const text = message.trim();
  const capital = text.charAt(0).toUpperCase() + text.slice(1);
  return /[.!?]$/.test(capital) ? capital : `${capital}.`;
}
