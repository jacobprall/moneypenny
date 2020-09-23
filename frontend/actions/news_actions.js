import fetchBusinessNews from '../util/news_util'

export const RECEIVE_NEWS = "RECEIVE_NEWS";

const receiveNews = news => ({
  type: RECEIVE_NEWS,
  news,
});


export const requestBusinessNews = () => dispatch => (
  fetchBusinessNews().then(
    response => dispatch(receiveNews(response))
  )
);
