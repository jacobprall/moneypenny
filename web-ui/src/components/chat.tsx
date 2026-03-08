import { useCallback, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { chat, type ChatResponse } from "@/lib/api";
import { cn } from "@/lib/utils";

export interface Message {
  id: string;
  role: "user" | "assistant";
  content: string;
  sessionId?: string;
}

export function Chat() {
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const sessionIdRef = useRef<string | null>(null);

  const sendMessage = useCallback(async () => {
    const text = input.trim();
    if (!text || loading) return;

    setInput("");
    setError(null);
    const userMsg: Message = {
      id: crypto.randomUUID(),
      role: "user",
      content: text,
    };
    setMessages((prev) => [...prev, userMsg]);
    setLoading(true);

    try {
      const res: ChatResponse = await chat({
        message: text,
        session_id: sessionIdRef.current ?? undefined,
      });
      sessionIdRef.current = res.session_id;
      const assistantMsg: Message = {
        id: crypto.randomUUID(),
        role: "assistant",
        content: res.response,
        sessionId: res.session_id,
      };
      setMessages((prev) => [...prev, assistantMsg]);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Request failed");
    } finally {
      setLoading(false);
    }
  }, [input, loading]);

  return (
    <Card className="flex h-full flex-col overflow-hidden">
      <CardHeader className="border-b py-3">
        <h1 className="text-lg font-semibold">Moneypenny</h1>
        <p className="text-muted-foreground text-sm">
          Chat with your agent. Session is preserved across messages.
        </p>
      </CardHeader>
      <CardContent className="flex min-h-0 flex-1 flex-col gap-3 p-0">
        <ScrollArea className="flex-1 px-4">
          <div className="flex flex-col gap-4 py-4">
            {messages.length === 0 && !loading && (
              <p className="text-muted-foreground text-center text-sm">
                Send a message to start.
              </p>
            )}
            {messages.map((m) => (
              <div
                key={m.id}
                className={cn(
                  "rounded-lg px-3 py-2",
                  m.role === "user"
                    ? "ml-8 bg-primary text-primary-foreground"
                    : "mr-8 bg-muted"
                )}
              >
                <div className="text-xs font-medium opacity-80">
                  {m.role === "user" ? "You" : "Agent"}
                </div>
                <div className="whitespace-pre-wrap break-words text-sm">
                  {m.content}
                </div>
              </div>
            ))}
            {loading && (
              <div className="mr-8 rounded-lg bg-muted px-3 py-2">
                <div className="text-xs font-medium text-muted-foreground">
                  Agent
                </div>
                <div className="text-sm text-muted-foreground">Thinking…</div>
              </div>
            )}
          </div>
        </ScrollArea>
        {error && (
          <div className="border-t bg-destructive/10 px-4 py-2 text-sm text-destructive">
            {error}
          </div>
        )}
        <div className="flex gap-2 border-t p-4">
          <Input
            placeholder="Type a message…"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && !e.shiftKey && sendMessage()}
            disabled={loading}
            className="flex-1"
          />
          <Button onClick={sendMessage} disabled={loading || !input.trim()}>
            Send
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
