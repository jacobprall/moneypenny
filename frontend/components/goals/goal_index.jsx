import React from 'react'
import GoalLineItem from './goal_line_item'
import GoalFormContainer from './goal_form_container'
import { useSelector, useDispatch } from 'react-redux'
import { openModal } from '../../actions/modal_actions'


export default function goal_index() {
  const dispatch = useDispatch();
  const goals = useSelector((state) => Object.values(state.entities.goals))
  const modalOpener = (formType, component, payload) => dispatch(openModal(formType, component, payload));
  const baseAccount = useSelector((state) => Object.values(state.entities.accounts)[0])

  let accountId = {};
  if (baseAccount) {
    accountId = baseAccount.id;
  }

  const newGoal = {
    'title': '',
    'goal_amount': 0,
    'goal_category': 'Other',
    'account_id': `${accountId}`
  }


  const renderGoals = () => (
    goals.map((goal, i) => (
      <GoalLineItem goal={goal} key={i} />
    ))
  );


  return (
    <div className="goals-index-container">
      <div className="goals">
        <div className="goal-header">
          Your Goals
        </div>
        <div className="new-goal-line-item">
          <button className="add-goal" onClick={() => modalOpener('new', GoalFormContainer, newGoal)}>+ Add Goal</button>
        </div>
        <div className="goals-list-container">
          <ul className="goals-list">
            {renderGoals()}
          </ul>
        </div>
      </div>
    </div>
  )
}
