export default function fetchBusinessNews() {
  return (
  $.ajax({
    url: 'https://newsapi.org/v2/top-headlines?country=us&apiKey=c52b1b6b03304f4b89a6fbfc26a4b2d5'
  })
  )
};