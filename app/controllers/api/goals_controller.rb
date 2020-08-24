class Api::GoalsController < ApplicationController
  def index 
    @goals = current_user.goals
    render :index 
  end

  def create
    @goal = Goal.create(goal_params)
    if @goal.save
      render 'api/goals/update'
    else
      render json: @goal.errors.full_messages, status: 422
    end

  end

  def update
    @goal = Transaction.find(params[:id])
    if @goal.update(goal_params)
      render 'api/goals/update'
    else
       render json: @goal.errors.full_messages, status: 422
    end

  end

  def destroy
    @goal = current_user.goals.find(params[:id])
    @goal.destroy 
    @goals = current_user.goals
    render json: @goal.id
  end

  def goal_params
    params.require(:goal).permit(:goal_amount, :goal_category, :title, :account_id)
  end
  
end
