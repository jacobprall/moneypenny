import { Chat } from "@/components/chat"

function App() {
  return (
    <div className="flex h-dvh w-full flex-col bg-background">
      <main className="flex flex-1 flex-col overflow-hidden p-4 md:mx-auto md:max-w-2xl md:p-6">
        <Chat />
      </main>
    </div>
  )
}

export default App
