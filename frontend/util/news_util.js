export default function fetchBusinessNews() {
  return (
  $.ajax({
    url: 'http://newsapi.org/v2/top-headlines?country=us&category=business&apiKey=c52b1b6b03304f4b89a6fbfc26a4b2d5'
  })
  )
}