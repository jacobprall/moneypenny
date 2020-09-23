import React from 'react'
import {formatDate} from '../../util/date_util'
export default function news_component({article}) {
  const {url, title, byline, imageUrl, createdDate} = article
  const formattedDate = formatDate(createdDate)

  return (
    <li className="news-list-item" onClick={() => window.open(`${url}`, "_blank")}>
      <img className="news-image" src={imageUrl.url} alt="image"/>
      <div className="article-content">
        <div className="article-title">{title}</div>
        <div className="article-author">{byline}</div>
        <div className="article-date">{formattedDate}</div>
      </div>
    </li>
  )
}
