export default function fetchBusinessNews() {
  return (
  $.ajax({
    url: 'https://api.nytimes.com/svc/topstories/v2/business.json?api-key=5ZAXAlQ3pBfCdaNrrQM6wAlDADXrodM5'
  })
  )
};