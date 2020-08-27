import React from 'react'
import {formatDate} from '../../util/date_util'
export default function news_component({article}) {

 const formattedDate = formatDate(article.date)
  // const handleClick = (e) => 
  return (
    <li className="news-list-item" onClick={() => window.open(`${article.url}`, "_blank")}>
      <img className="news-image" src={article.imageUrl} alt="image"/>
      <div className="article-content">
        <div className="article-title">{article.title}</div>
        <div className="article-date">{formattedDate}</div>
      </div>
    </li>
  )
}
