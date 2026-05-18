import { memo } from 'react'
import ReactMarkdown from 'react-markdown'
import rehypeHighlight from 'rehype-highlight'
import remarkGfm from 'remark-gfm'
import '@/components/session/messages/assistant.css'

export const AssistantMessage = memo(function AssistantMessage(props: {
  messageId: string
  content: string
}) {
  return (
    <div className="max-w-none font-sans text-base leading-relaxed text-fg">
      <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeHighlight]}>
        {props.content}
      </ReactMarkdown>
    </div>
  )
})
