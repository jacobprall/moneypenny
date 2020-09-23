import React from "react";
import { useSelector, shallowEqual } from "react-redux";
import NewsComponent from "./news_component";

export default function news_component_container() {
  const articleResponseArray = useSelector(
    (state) => Object.values(state.entities.news),
    shallowEqual
  );

  const formattedArticlesArray = () => {
    if (articleResponseArray.length > 0) {
      const formattedArticles = articleResponseArray[5].map((rawArticle, i) => {
        if (rawArticle.multimedia) {
          return {
            url: rawArticle.short_url,
            title: rawArticle.title,
            byline: rawArticle.byline,
            imageUrl: rawArticle.multimedia[4],
            createdDate: rawArticle.created_date,
          };
        }
      });
      return formattedArticles;
    }
    return [];
  };

  const renderNews = () => {
    let articles = formattedArticlesArray().map((article, i) => {
      while (i < 10) {
        return <NewsComponent article={article} key={i} />;
      }
    });
    return articles;
  };

  return <ul className="news-list">{renderNews()}</ul>;
}
