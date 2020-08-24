import React, { useState } from 'react'
import { Doughnut } from 'react-chartjs-2'


export default function chart(data) {

  const [data, setData] = useState()
  
  setData({
    datasets: [
      {
        data: [10, 20, 30],
        label: 'Test',
      },
    ],
    labels: ['red', 'yellow', 'blue']
  })

  return (
    <div className="chart">
      <Doughnut 
        data={data}
        options={{
          title:{
            display: true,
            text: "Test",
            fontSize: 20
          },
          legend: {
            display: true,
            position: 'right'
          }
        }}
      />
    </div>
  )
}
