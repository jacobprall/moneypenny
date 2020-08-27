import fetchBusinessNews from '../util/news_util'

export const RECEIVE_NEWS = "RECEIVE_NEWS";
export const RECEIVE_NEWS_ERRORS = "RECEIVE_NEWS_ERRORS";
export const START_LOADING_NEWS = "START_LOADING_NEWS";

const receiveNews = news => ({
  type: RECEIVE_NEWS,
  news,
});

const receiveNewsErrors = errs => ({
  type: RECEIVE_NEWS_ERRORS,
  errs
});

const startLoadingNews = () => ({
  type: START_LOADING_NEWS,
});

export const requestBusinessNews = () => dispatch => (
  fetchBusinessNews().then(
    response => dispatch(receiveNews(response))
  )
);
