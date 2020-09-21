import React, { useState, useEffect } from "react";
import { Doughnut } from "react-chartjs-2";
import { useSelector, shallowEqual, useDispatch } from "react-redux";
import { formatDate } from "../../util/date_util";
import { requestTransactions } from "../../actions/transaction_actions";
export default function chart() {
  const getSelectedData = () =>
    useSelector((state) => state.entities.transactions);
  let selectedData = getSelectedData();
  while (selectedData.length === 0) {
    selectedData = getSelectedData();
  }

  const categoryAmountPairs = Object.values(selectedData).map((transaction) => [
    transaction.transaction_category,
    transaction.amount,
    transaction.date,
  ]);

  const todayMonth = formatDate(new Date()).split(" ")[0];

  const computeTransactionData = () => {
    const transactionObj = {};

    categoryAmountPairs.forEach((transaction) => {
      const month = formatDate(transaction[2]).split(" ")[0];
      if (
        transactionObj[transaction[0]] === undefined &&
        transaction[0] !== "Income" &&
        month === todayMonth
      ) {
        transactionObj[transaction[0]] = Math.abs(transaction[1]);
      } else if (transactionObj[transaction[0]] && month === todayMonth) {
        transactionObj[transaction[0]] += Math.abs(transaction[1]);
      }
    });
    const labels = [];
    const transactionTotals = [];
    for (const transaction in transactionObj) {
      labels.push(transaction);
      transactionTotals.push(transactionObj[transaction]);
    }
    return [labels, transactionTotals];
  };

  let [labels, data] = computeTransactionData();

  const dataset = {
    datasets: [
      {
        data: data,
        label: "Spending By Category",
        backgroundColor: [
          "#FECF13",
          "#D98C23",
          "#33CCE1",
          "#a9dae1",
          "#F79ED9",
          "#FF43CE",
          "#C9E974",
          "#EFFF88",
        ],
        hoverBackgroundColor: [
          "#e4ba11",
          "#ad701c",
          "#28a3b4",
          "#7ec7d2",
          "#de8ec3",
          "#cc35a4",
          "#a0ba5c",
          "#bfcc6c",
        ],
      },
    ],
    labels: labels,
  };

  return (
    <div className="chart">
      <Doughnut
        data={dataset}
        options={{
          title: {
            display: true,
            text: "Spending By Category",
            fontSize: 40,
          },
          subtitles: [
            {
              text: `${todayMonth}`,
              fontSize: 18,
              verticalAlign: "center",
              dockInsidePlotArea: true,
            },
          ],
          legend: {
            display: true,
            position: "right",
          },
        }}
      />
    </div>
  );
}
