import React, {useState} from 'react'
import { useSelector, shallowEqual, useDispatch } from 'react-redux'
import { requestBusinessNews } from '../../actions/news_actions'
import NewsComponent from './news_component'

export default function news_component_container() {
  const rawArticlesArray = useSelector((state) =>(state.entities.news), shallowEqual)

  
  const formattedArticlesArray = rawArticlesArray.map((rawArticle, i) => {
    return { source: rawArticle.source.name, 
      author: rawArticle.author, 
      description: rawArticle.description, 
      date: rawArticle.publishedAt, 
      title: rawArticle.title, 
      url: rawArticle.url, 
      imageUrl: rawArticle.urlToImage }
    }
  );
  console.log(formattedArticlesArray)

  const renderNews = () => {
    let articles = formattedArticlesArray.map((article, i) => {
      while (i < 10) {
        console.log(article)
        return (
          <NewsComponent article={article} key={i} />
        )
      }
    })
    return articles
  }




  return (
    <ul className="news-list">
      {renderNews()}
    </ul>
  )
}
